use hashbrown::HashMap;

use crate::ast::Ast;
use crate::error::Error;
use crate::parser;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module};
use libm::{
    acos, acosh, asin, asinh, atan, atanh, cbrt, ceil, cos, cosh, exp, fabs, floor, log, log10,
    log2, pow, sin, sinh, sqrt, tan, tanh,
};
use rayon::prelude::*;
use std::mem;
use std::slice;

const POW: &str = "pow";

/// Host-callable batch evaluator produced by [`JIT::compile`].
pub type BatchFunction =
    dyn Fn(&HashMap<String, &[f64]>, usize) -> Result<Vec<f64>, Error> + Send + Sync;

/// Host math symbols exposed to JIT-compiled expressions.
fn math_symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        (POW, pow as *const u8),
        ("sqrt", sqrt as *const u8),
        ("cbrt", cbrt as *const u8),
        ("sin", sin as *const u8),
        ("cos", cos as *const u8),
        ("tan", tan as *const u8),
        ("asin", asin as *const u8),
        ("acos", acos as *const u8),
        ("atan", atan as *const u8),
        ("sinh", sinh as *const u8),
        ("cosh", cosh as *const u8),
        ("tanh", tanh as *const u8),
        ("asinh", asinh as *const u8),
        ("acosh", acosh as *const u8),
        ("atanh", atanh as *const u8),
        ("floor", floor as *const u8),
        ("ceil", ceil as *const u8),
        ("abs", fabs as *const u8),
        ("exp", exp as *const u8),
        ("ln", log as *const u8),
        ("log2", log2 as *const u8),
        ("log10", log10 as *const u8),
    ]
}

/// The basic JIT class.
pub struct JIT {
    /// The function builder context, which is reused across multiple
    /// FunctionBuilder instances.
    builder_context: FunctionBuilderContext,

    /// The main Cranelift context, which holds the state for codegen.
    ctx: codegen::Context,

    /// The data description, which is to data objects what `ctx` is to functions.
    data_description: DataDescription,

    /// The module, with the jit backend, which manages the JIT'd functions.
    module: JITModule,

    /// The parameter names and their indexes for function calls
    required_parameters: HashMap<String, usize>,
}

impl Default for JIT {
    fn default() -> Self {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .expect("failed to create JITBuilder for host ISA");
        for (name, ptr) in math_symbols() {
            builder.symbol(name, ptr);
        }
        let module = JITModule::new(builder);
        Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_description: DataDescription::new(),
            module,
            required_parameters: HashMap::new(),
        }
    }
}

impl JIT {
    /// Create a new `JIT` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile a string expression into machine code.
    pub fn compile(&mut self, input: &str) -> Result<Box<BatchFunction>, Error> {
        let ast = parser::parse(input)?;

        self.required_parameters.clear();
        self.translate(&ast)?;

        let id = self
            .module
            .declare_function(input, Linkage::Export, &self.ctx.func.signature)
            .map_err(|e| Error::parse(e.to_string()))?;

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| Error::parse(e.to_string()))?;

        self.module.clear_context(&mut self.ctx);

        self.module
            .finalize_definitions()
            .map_err(|e| Error::parse(e.to_string()))?;

        let code = self.module.get_finalized_function(id);

        Ok(Self::dynamic_param_fn(code, &self.required_parameters))
    }

    fn get_sorted_keys(keys: &HashMap<String, usize>) -> Vec<String> {
        let mut sorted_keys = Vec::with_capacity(keys.len());
        for k in keys.keys() {
            sorted_keys.push(k.to_owned());
        }
        sorted_keys.sort_unstable();
        sorted_keys
    }

    fn dynamic_param_fn(
        function: *const u8,
        required_parameters: &HashMap<String, usize>,
    ) -> Box<BatchFunction> {
        let sorted_keys = Self::get_sorted_keys(required_parameters);
        // Specialized Rust fn-pointer ABIs for 0..=16 f64 params. This matches
        // Cranelift's system calling convention and avoids a packed-pointer path.
        match required_parameters.len() {
            0 => {
                let function = unsafe { mem::transmute::<*const u8, fn() -> f64>(function) };
                Box::new(
                    move |_params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results = Vec::with_capacity(number_of_evaluations);
                        results.extend((0..number_of_evaluations).map(|_| function()));
                        Ok(results)
                    },
                )
            }
            1 => {
                let function = unsafe { mem::transmute::<*const u8, fn(f64) -> f64>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let p0 = Self::require_col(params, &sorted_keys[0], n)?;
                    Ok(Self::map_batch(n, |i| function(p0[i])))
                })
            }
            2 => {
                let function =
                    unsafe { mem::transmute::<*const u8, fn(f64, f64) -> f64>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let p0 = Self::require_col(params, &sorted_keys[0], n)?;
                    let p1 = Self::require_col(params, &sorted_keys[1], n)?;
                    Ok(Self::map_batch(n, |i| function(p0[i], p1[i])))
                })
            }
            3 => {
                let function =
                    unsafe { mem::transmute::<*const u8, fn(f64, f64, f64) -> f64>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let p0 = Self::require_col(params, &sorted_keys[0], n)?;
                    let p1 = Self::require_col(params, &sorted_keys[1], n)?;
                    let p2 = Self::require_col(params, &sorted_keys[2], n)?;
                    Ok(Self::map_batch(n, |i| function(p0[i], p1[i], p2[i])))
                })
            }
            4 => {
                let function = unsafe {
                    mem::transmute::<*const u8, fn(f64, f64, f64, f64) -> f64>(function)
                };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let p0 = Self::require_col(params, &sorted_keys[0], n)?;
                    let p1 = Self::require_col(params, &sorted_keys[1], n)?;
                    let p2 = Self::require_col(params, &sorted_keys[2], n)?;
                    let p3 = Self::require_col(params, &sorted_keys[3], n)?;
                    Ok(Self::map_batch(n, |i| function(p0[i], p1[i], p2[i], p3[i])))
                })
            }
            5 => {
                type F = fn(f64, f64, f64, f64, f64) -> f64;
                let function = unsafe { mem::transmute::<*const u8, F>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let c = Self::require_cols(params, &sorted_keys, n)?;
                    Ok(Self::map_batch(n, |i| {
                        function(c[0][i], c[1][i], c[2][i], c[3][i], c[4][i])
                    }))
                })
            }
            6 => {
                type F = fn(f64, f64, f64, f64, f64, f64) -> f64;
                let function = unsafe { mem::transmute::<*const u8, F>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let c = Self::require_cols(params, &sorted_keys, n)?;
                    Ok(Self::map_batch(n, |i| {
                        function(c[0][i], c[1][i], c[2][i], c[3][i], c[4][i], c[5][i])
                    }))
                })
            }
            7 => {
                type F = fn(f64, f64, f64, f64, f64, f64, f64) -> f64;
                let function = unsafe { mem::transmute::<*const u8, F>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let c = Self::require_cols(params, &sorted_keys, n)?;
                    Ok(Self::map_batch(n, |i| {
                        function(
                            c[0][i], c[1][i], c[2][i], c[3][i], c[4][i], c[5][i], c[6][i],
                        )
                    }))
                })
            }
            8 => {
                type F = fn(f64, f64, f64, f64, f64, f64, f64, f64) -> f64;
                let function = unsafe { mem::transmute::<*const u8, F>(function) };
                Box::new(move |params: &HashMap<String, &[f64]>, n: usize| {
                    let c = Self::require_cols(params, &sorted_keys, n)?;
                    Ok(Self::map_batch(n, |i| {
                        function(
                            c[0][i], c[1][i], c[2][i], c[3][i], c[4][i], c[5][i], c[6][i],
                            c[7][i],
                        )
                    }))
                })
            }
            n if n <= 16 => {
                // General path for 9..=16: pack into a stack array and call via
                // a Cranelift kernel that still uses N scalar params — host side
                // uses an adapter that cannot transmute easily past 8, so we
                // reject at compile time above 8 for now.
                Box::new(move |_params: &HashMap<String, &[f64]>, _n: usize| {
                    Err(Error::parse(format!(
                        "JIT supports at most 8 parameters (got {})",
                        sorted_keys.len()
                    )))
                })
            }
            n => Box::new(move |_params: &HashMap<String, &[f64]>, _n: usize| {
                Err(Error::parse(format!(
                    "JIT supports at most 8 parameters (got {})",
                    n
                )))
            }),
        }
    }

    #[inline]
    fn require_col<'a>(
        params: &'a HashMap<String, &[f64]>,
        key: &str,
        n: usize,
    ) -> Result<&'a [f64], Error> {
        let col = params
            .get(key)
            .ok_or_else(|| Error::name(format!("Missing parameter: {}", key)))?;
        if n > col.len() {
            return Err(Error::name(format!("Missing data for parameter: {}", key)));
        }
        Ok(col)
    }


    fn require_cols<'a>(
        params: &'a HashMap<String, &[f64]>,
        keys: &[String],
        n: usize,
    ) -> Result<Vec<&'a [f64]>, Error> {
        keys.iter()
            .map(|k| Self::require_col(params, k, n))
            .collect()
    }

    /// Fill a result buffer, using rayon when the batch is large enough to amortize overhead.
    #[inline]
    fn map_batch<F>(n: usize, f: F) -> Vec<f64>
    where
        F: Fn(usize) -> f64 + Sync + Send,
    {
        // Threshold chosen so tiny benches stay single-threaded while multi-million
        // Monte Carlo columns can use all cores.
        const PARALLEL_THRESHOLD: usize = 16_384;
        if n >= PARALLEL_THRESHOLD {
            (0..n).into_par_iter().map(f).collect()
        } else {
            let mut results = Vec::with_capacity(n);
            results.extend((0..n).map(f));
            results
        }
    }

    /// Create a zero-initialized data section.
    pub fn create_data(&mut self, name: &str, contents: Vec<u8>) -> Result<&[u8], Error> {
        self.data_description.define(contents.into_boxed_slice());
        let id = self
            .module
            .declare_data(name, Linkage::Export, true, false)
            .map_err(|e| Error::name(e.to_string()))?;

        self.module
            .define_data(id, &self.data_description)
            .map_err(|e| Error::parse(e.to_string()))?;
        self.data_description.clear();
        self.module
            .finalize_definitions()
            .map_err(|e| Error::parse(e.to_string()))?;
        let buffer = self.module.get_finalized_data(id);
        Ok(unsafe { slice::from_raw_parts(buffer.0, buffer.1) })
    }

    fn get_parameters<'a>(ast: &'a Ast, context: &mut Vec<&'a str>) {
        match ast {
            Ast::Variable(name) => {
                context.push(name);
            }
            Ast::Value(_) => {}
            Ast::Function(_, ref arg) => {
                Self::get_parameters(arg, context);
            }
            Ast::Add(ref left, ref right)
            | Ast::Sub(ref left, ref right)
            | Ast::Mul(ref left, ref right)
            | Ast::Div(ref left, ref right)
            | Ast::Exp(ref left, ref right) => {
                Self::get_parameters(left, context);
                Self::get_parameters(right, context);
            }
        }
    }

    fn translate(&mut self, ast: &Ast) -> Result<(), Error> {
        let mut parameter_names = vec![];
        Self::get_parameters(ast, &mut parameter_names);
        parameter_names.sort_unstable();
        parameter_names.dedup();
        for (i, param_name) in parameter_names.iter().enumerate() {
            self.required_parameters.insert((*param_name).to_owned(), i);
        }

        if parameter_names.len() > 8 {
            return Err(Error::parse(format!(
                "JIT supports at most 8 parameters (got {})",
                parameter_names.len()
            )));
        }
        for _p in &parameter_names {
            self.ctx
                .func
                .signature
                .params
                .push(AbiParam::new(types::F64));
        }

        self.ctx
            .func
            .signature
            .returns
            .push(AbiParam::new(types::F64));

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);

        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let variables = declare_variables(&mut builder, &parameter_names, entry_block);

        let mut trans = FunctionTranslator {
            builder,
            variables,
            module: &mut self.module,
        };

        let expression_val = trans.translate_expr(ast);
        trans.builder.ins().return_(&[expression_val]);
        let target_config = trans.module.target_config();
        trans.builder.finalize(target_config);
        Ok(())
    }
}

/// A collection of state used for translating AST nodes into Cranelift IR.
struct FunctionTranslator<'a> {
    builder: FunctionBuilder<'a>,
    variables: HashMap<String, Variable>,
    module: &'a mut JITModule,
}

impl<'a> FunctionTranslator<'a> {
    fn translate_expr(&mut self, ast: &Ast) -> Value {
        match *ast {
            Ast::Value(val) => self.builder.ins().f64const(Ieee64::with_float(val)),
            Ast::Variable(ref name) => {
                let variable = self.variables.get(name).expect("variable not defined");
                self.builder.use_var(*variable)
            }
            Ast::Add(ref left, ref right) => {
                let lhs = self.translate_expr(left);
                let rhs = self.translate_expr(right);
                self.builder.ins().fadd(lhs, rhs)
            }
            Ast::Sub(ref left, ref right) => {
                let lhs = self.translate_expr(left);
                let rhs = self.translate_expr(right);
                self.builder.ins().fsub(lhs, rhs)
            }
            Ast::Mul(ref left, ref right) => {
                let lhs = self.translate_expr(left);
                let rhs = self.translate_expr(right);
                self.builder.ins().fmul(lhs, rhs)
            }
            Ast::Div(ref left, ref right) => {
                let lhs = self.translate_expr(left);
                let rhs = self.translate_expr(right);
                self.builder.ins().fdiv(lhs, rhs)
            }
            Ast::Exp(ref left, ref right) => {
                let lhs = self.translate_expr(left);
                let rhs = self.translate_expr(right);
                self.translate_call(POW, &[lhs, rhs])
            }
            Ast::Function(ref name, ref arg) => {
                let arg_val = self.translate_expr(arg);
                self.translate_call(name, &[arg_val])
            }
        }
    }

    fn translate_call(&mut self, name: &str, args: &[Value]) -> Value {
        let mut sig = self.module.make_signature();

        for _arg in args {
            sig.params.push(AbiParam::new(types::F64));
        }
        sig.returns.push(AbiParam::new(types::F64));

        let callee = self
            .module
            .declare_function(name, Linkage::Import, &sig)
            .expect("problem declaring function");
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);
        let call = self.builder.ins().call(local_callee, args);
        self.builder.inst_results(call)[0]
    }
}

fn declare_variables(
    builder: &mut FunctionBuilder,
    params: &[&str],
    entry_block: Block,
) -> HashMap<String, Variable> {
    let mut variables = HashMap::new();

    for (i, name) in params.iter().enumerate() {
        let value = builder.block_params(entry_block)[i];
        let var = declare_variable(builder, &mut variables, name);
        builder.def_var(var, value);
    }

    variables
}

fn declare_variable(
    builder: &mut FunctionBuilder,
    variables: &mut HashMap<String, Variable>,
    name: &str,
) -> Variable {
    *variables
        .entry(name.into())
        .or_insert_with(|| builder.declare_var(types::F64))
}

#[cfg(test)]
mod tests {
    use super::HashMap;
    use crate::jit::JIT;

    #[test]
    fn compile_constants_and_ops() {
        let mut jit = JIT::new();
        let f = jit.compile("2+2").unwrap();
        let params = HashMap::new();
        assert_eq!(f(&params, 1).unwrap()[0], 4.0);

        let f = jit.compile("3 * 5 + 1").unwrap();
        assert_eq!(f(&params, 1).unwrap()[0], 16.0);
    }

    #[test]
    fn compile_math_functions() {
        let mut jit = JIT::new();
        let params = HashMap::new();

        let f = jit.compile("sqrt(9)").unwrap();
        assert!((f(&params, 1).unwrap()[0] - 3.0).abs() < 1e-10);

        let f = jit.compile("sin(0)").unwrap();
        assert!((f(&params, 1).unwrap()[0]).abs() < 1e-10);

        let f = jit.compile("abs(-7)").unwrap();
        assert!((f(&params, 1).unwrap()[0] - 7.0).abs() < 1e-10);

        let f = jit.compile("2 ^ 3").unwrap();
        assert!((f(&params, 1).unwrap()[0] - 8.0).abs() < 1e-10);
    }

    #[test]
    fn compile_with_variables() {
        let mut jit = JIT::new();
        let f = jit.compile("x + y * 2").unwrap();
        let mut owned: HashMap<String, Vec<f64>> = HashMap::new();
        owned.insert("x".into(), vec![1.0, 2.0]);
        owned.insert("y".into(), vec![3.0, 4.0]);
        let params: HashMap<String, &[f64]> = owned
            .iter()
            .map(|(k, v)| (k.clone(), v.as_slice()))
            .collect();
        let results = f(&params, 2).unwrap();
        assert_eq!(results[0], 7.0);
        assert_eq!(results[1], 10.0);
    }

    #[test]
    fn compile_five_plus_params() {
        let mut jit = JIT::new();
        let f = jit.compile("a + b + c + d + e").unwrap();
        let mut owned: HashMap<String, Vec<f64>> = HashMap::new();
        owned.insert("a".into(), vec![1.0]);
        owned.insert("b".into(), vec![2.0]);
        owned.insert("c".into(), vec![3.0]);
        owned.insert("d".into(), vec![4.0]);
        owned.insert("e".into(), vec![5.0]);
        let params: HashMap<String, &[f64]> = owned
            .iter()
            .map(|(k, v)| (k.clone(), v.as_slice()))
            .collect();
        let results = f(&params, 1).unwrap();
        assert!((results[0] - 15.0).abs() < 1e-12);
    }

    #[test]
    #[ignore = "heavy micro-benchmark; use criterion benches instead"]
    fn bench() {
        use crate::jit::BatchFunction;
        use std::process;
        use std::time::Instant;

        let watch = Instant::now();
        let mut jit = super::JIT::new();

        let compiled_formulas: Vec<Box<BatchFunction>> = vec![
            "(var1 + var2 * 3) / (2 + 3) - something",
            "(var1 - var2 / 3) * (2 - 3) + something",
            "(var1 - var2 + 3) / (2 / 3) + something",
        ]
        .into_iter()
        .map(|formula_string| {
            jit.compile(formula_string).unwrap_or_else(|msg| {
                dbg!(msg);
                process::exit(1);
            })
        })
        .collect();

        let mut dict: HashMap<String, Vec<f64>> = HashMap::with_capacity(3);
        let capacity = 5_000_000;
        let iterations = 5_000_000;
        dict.insert("var1".to_owned(), Vec::with_capacity(capacity));
        dict.insert("var2".to_owned(), Vec::with_capacity(capacity));
        dict.insert("something".to_owned(), Vec::with_capacity(capacity));
        for i in 1..=iterations {
            dict.get_mut("var1").unwrap().push(10.0 + f64::from(i));
            dict.get_mut("var2").unwrap().push(20.0 + f64::from(i));
            dict.get_mut("something").unwrap().push(30.0 + f64::from(i));
        }
        let dict2: HashMap<String, &[f64]> = dict
            .iter_mut()
            .map(|v| (v.0.to_owned(), v.1.as_slice()))
            .collect();
        let watch = watch.elapsed();

        let mut results = vec![];
        let watch2 = Instant::now();
        for compiled_formula in compiled_formulas {
            let result = compiled_formula(&dict2, capacity);
            results.push(result);
        }
        let watch2 = watch2.elapsed();

        match &results[0] {
            Ok(results) => println!("{}", results[0]),
            Err(msg) => println!("{}", msg),
        }
        println!("{}", watch.as_millis());
        println!("{}", watch2.as_millis());
    }
}
