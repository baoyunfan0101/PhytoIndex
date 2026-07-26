use rhai::{AST, CallFnOptions, Dynamic, Engine, ImmutableString, Scope};
use serde::de::DeserializeOwned;

use super::normalize_taxonomy_name;
use crate::metadata::{self, MetadataKey};
use crate::{CoreError, CoreResult};

#[cfg(test)]
std::thread_local! {
    static COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) struct CompiledHook {
    engine: Engine,
    ast: AST,
}

impl CompiledHook {
    pub(super) fn new(script: &str) -> CoreResult<Self> {
        #[cfg(test)]
        COMPILE_COUNT.set(COMPILE_COUNT.get() + 1);
        if script.len() > 65_536 {
            return Err(invalid_hook("script exceeds 64 KiB".into()));
        }
        let mut engine = Engine::new();
        engine.on_print(|_| {});
        engine.on_debug(|_, _, _| {});
        engine.set_max_operations(20_000);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_functions(16);
        engine.set_max_variables(64);
        engine.set_max_string_size(16_384);
        engine.set_max_array_size(64);
        engine.set_max_map_size(32);
        engine.register_fn("normalize_name", |value: ImmutableString| {
            normalize_taxonomy_name(&value).unwrap_or_default()
        });
        engine.register_fn("is_uppercase", char::is_uppercase);
        engine.register_fn("is_whitespace", char::is_whitespace);
        let ast = engine
            .compile(script)
            .map_err(|error| invalid_hook(error.to_string()))?;
        Ok(Self { engine, ast })
    }

    pub(super) fn call<T: DeserializeOwned>(&self, function: &str, input: &str) -> CoreResult<T> {
        let mut scope = Scope::new();
        let output = self
            .engine
            .call_fn_with_options::<Dynamic>(
                CallFnOptions::new().eval_ast(false),
                &mut scope,
                &self.ast,
                function,
                (input.to_string(),),
            )
            .map_err(|error| invalid_hook(error.to_string()))?;
        rhai::serde::from_dynamic(&output).map_err(|error| invalid_hook(error.to_string()))
    }
}

#[cfg(test)]
pub(crate) fn take_compile_count() -> usize {
    COMPILE_COUNT.replace(0)
}

pub(super) fn load_script(
    connection: &rusqlite::Connection,
    key: MetadataKey,
) -> CoreResult<Option<String>> {
    metadata::get_raw(connection, key)
}

fn invalid_hook(message: String) -> CoreError {
    CoreError::InvalidArgument(format!("invalid naming hook: {message}"))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Output {
        value: String,
    }

    #[test]
    fn calls_functions_without_evaluating_the_ast() {
        let hook = CompiledHook::new(
            r#"
            throw "top-level evaluation must stay disabled";
            fn run(input) {
                #{ value: input }
            }
            "#,
        )
        .unwrap();

        let output = hook.call::<Output>("run", "raw").unwrap();
        assert_eq!(output.value, "raw");
    }
}
