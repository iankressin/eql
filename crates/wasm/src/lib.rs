use eql_core::interpreter::eql as eql_interpreter;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn eql(program: &str) -> Result<JsValue, JsValue> {
    let result = eql_interpreter(program).await;

    match result {
        Ok(result) => {
            let result = serde_wasm_bindgen::to_value(&result)?;
            return Ok(result);
        }
        Err(e) => {
            return Err(JsValue::from_str(&e.to_string()));
        }
    }
}
