use rhai::{AST, Dynamic, Engine, ImmutableString, Scope};
use serde::de::DeserializeOwned;

use super::normalize_taxonomy_name;
use crate::{CoreError, CoreResult};

pub(super) struct CompiledHook {
    engine: Engine,
    ast: AST,
}

impl CompiledHook {
    pub(super) fn new(script: &str) -> CoreResult<Self> {
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
            .call_fn::<Dynamic>(&mut scope, &self.ast, function, (input.to_string(),))
            .map_err(|error| invalid_hook(error.to_string()))?;
        rhai::serde::from_dynamic(&output).map_err(|error| invalid_hook(error.to_string()))
    }
}

pub(super) fn load_script(
    connection: &rusqlite::Connection,
    key: &str,
) -> CoreResult<Option<String>> {
    use rusqlite::OptionalExtension;

    connection
        .query_row(
            "SELECT metadata_value FROM app_metadata WHERE metadata_key = ?",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

fn invalid_hook(message: String) -> CoreError {
    CoreError::InvalidArgument(format!("invalid naming hook: {message}"))
}
