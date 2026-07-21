use hashbrown::HashMap;

use crate::ast::Ast;
use crate::error::Error;
use crate::lexer::Lexer;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module};
use libm::{
    acos, acosh, asin, asinh, atan, atanh, cbrt, ceil, cos, cosh, exp, fabs, floor, log, log10,
    log2, pow, sin, sinh, sqrt, tan, tanh,
};
use std::mem;
use std::slice;

const POW: &str = "pow";

type ReturnFunction = dyn Fn(&HashMap<String, &[f64]>, usize) -> Result<Vec<f64>, Error>;

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
    #[must_use]
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
    #[must_use]
    /// Create a new `JIT` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile a string expression into machine code.
    pub fn compile(&mut self, input: &str) -> Result<Box<ReturnFunction>, Error> {
        let mut lexer = Lexer::new(input);
        let ast = Ast::from_tokens(
            &mut lexer
                .parse()
                .map_err(|e| Error::ParseError(e.to_string()))?,
            "",
        )
        .map_err(|e| Error::ParseError(e.to_string()))?;

        self.required_parameters.clear();
        self.translate(&ast)?;

        let id = self
            .module
            .declare_function(input, Linkage::Export, &self.ctx.func.signature)
            .map_err(|e| Error::ParseError(e.to_string()))?;

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| Error::ParseError(e.to_string()))?;

        self.module.clear_context(&mut self.ctx);

        self.module
            .finalize_definitions()
            .map_err(|e| Error::ParseError(e.to_string()))?;

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
    ) -> Box<ReturnFunction> {
        let sorted_keys = Self::get_sorted_keys(required_parameters);
        match required_parameters.len() {
            0 => {
                let function = unsafe { mem::transmute::<_, fn() -> f64>(function) };
                Box::new(
                    move |_params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results: Vec<f64> = Vec::with_capacity(number_of_evaluations);
                        for _i in 0..number_of_evaluations {
                            results.push(function());
                        }
                        Ok(results)
                    },
                )
            }
            1 => {
                let function = unsafe { mem::transmute::<_, fn(f64) -> f64>(function) };
                Box::new(
                    move |params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results: Vec<f64> = Vec::with_capacity(number_of_evaluations);
                        let param_1 = params.get(&sorted_keys[0]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[0]))
                        })?;
                        for k in &sorted_keys {
                            if number_of_evaluations > params[k].len() {
                                return Err(Error::NameError(format!(
                                    "Missing data for parameter: {}",
                                    k
                                )));
                            }
                        }
                        for i in 0..number_of_evaluations {
                            results.push(function(param_1[i]));
                        }
                        Ok(results)
                    },
                )
            }
            2 => {
                let function = unsafe { mem::transmute::<_, fn(f64, f64) -> f64>(function) };
                Box::new(
                    move |params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results: Vec<f64> = Vec::with_capacity(number_of_evaluations);
                        let param_1 = params.get(&sorted_keys[0]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[0]))
                        })?;
                        let param_2 = params.get(&sorted_keys[1]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[1]))
                        })?;
                        for k in &sorted_keys {
                            if number_of_evaluations > params[k].len() {
                                return Err(Error::NameError(format!(
                                    "Missing data for parameter: {}",
                                    k
                                )));
                            }
                        }
                        for i in 0..number_of_evaluations {
                            results.push(function(param_1[i], param_2[i]));
                        }
                        Ok(results)
                    },
                )
            }
            3 => {
                let function = unsafe { mem::transmute::<_, fn(f64, f64, f64) -> f64>(function) };
                Box::new(
                    move |params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results: Vec<f64> = Vec::with_capacity(number_of_evaluations);
                        let param_1 = params.get(&sorted_keys[0]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[0]))
                        })?;
                        let param_2 = params.get(&sorted_keys[1]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[1]))
                        })?;
                        let param_3 = params.get(&sorted_keys[2]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[2]))
                        })?;
                        for k in &sorted_keys {
                            if number_of_evaluations > params[k].len() {
                                return Err(Error::NameError(format!(
                                    "Missing data for parameter: {}",
                                    k
                                )));
                            }
                        }
                        for i in 0..number_of_evaluations {
                            results.push(function(param_1[i], param_2[i], param_3[i]));
                        }
                        Ok(results)
                    },
                )
            }
            4 => {
                let function =
                    unsafe { mem::transmute::<_, fn(f64, f64, f64, f64) -> f64>(function) };
                Box::new(
                    move |params: &HashMap<String, &[f64]>, number_of_evaluations: usize| {
                        let mut results: Vec<f64> = Vec::with_capacity(number_of_evaluations);
                        let param_1 = params.get(&sorted_keys[0]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[0]))
                        })?;
                        let param_2 = params.get(&sorted_keys[1]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[1]))
                        })?;
                        let param_3 = params.get(&sorted_keys[2]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[2]))
                        })?;
                        let param_4 = params.get(&sorted_keys[3]).ok_or_else(|| {
                            Error::NameError(format!("Missing parameter: {}", &sorted_keys[3]))
                        })?;
                        for k in &sorted_keys {
                            if number_of_evaluations > params[k].len() {
                                return Err(Error::NameError(format!(
                                    "Missing data for parameter: {}",
                                    k
                                )));
                            }
                        }
                        for i in 0..number_of_evaluations {
                            results.push(function(param_1[i], param_2[i], param_3[i], param_4[i]));
                        }
                        Ok(results)
                    },
                )
            }
            _ => panic!("JIT currently supports at most 4 parameters"),
        }
    }

    /// Create a zero-initialized data section.
    pub fn create_data(&mut self, name: &str, contents: Vec<u8>) -> Result<&[u8], Error> {
        self.data_description.define(contents.into_boxed_slice());
        let id = self
            .module
            .declare_data(name, Linkage::Export, true, false)
            .map_err(|e| Error::NameError(e.to_string()))?;

        self.module
            .define_data(id, &self.data_description)
            .map_err(|e| Error::ParseError(e.to_string()))?;
        self.data_description.clear();
        self.module
            .finalize_definitions()
            .map_err(|e| Error::ParseError(e.to_string()))?;
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

        let return_variable = trans.variables[".the_return"];
        trans.builder.def_var(return_variable, expression_val);
        let return_value = trans.builder.use_var(return_variable);

        trans.builder.ins().return_(&[return_value]);
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
    let zero = builder.ins().f64const(Ieee64::with_float(0.0));
    let return_variable = declare_variable(builder, &mut variables, ".the_return");
    builder.def_var(return_variable, zero);

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
    #[ignore = "heavy micro-benchmark; use criterion benches instead"]
    fn bench() {
        use crate::jit::ReturnFunction;
        use std::process;
        use std::time::Instant;

        let watch = Instant::now();
        let mut jit = super::JIT::new();

        let compiled_formulas: Vec<Box<ReturnFunction>> = vec![
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
