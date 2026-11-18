use napi::{Env, Result, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use raad::sdk;
fn error(value: raad::Error) -> napi::Error {
    napi::Error::from_reason(sdk::error_json(value))
}

#[napi]
pub struct Repository {
    inner: sdk::Repository,
}
#[napi]
impl Repository {
    #[napi]
    pub fn call(&self, request: String, token: Option<String>) -> AsyncTask<Call> {
        AsyncTask::new(Call {
            inner: self.inner.clone(),
            request,
            token,
        })
    }
}
pub struct Open {
    options: String,
}
impl Task for Open {
    type Output = sdk::Repository;
    type JsValue = Repository;
    fn compute(&mut self) -> Result<Self::Output> {
        sdk::Repository::open(&self.options).map_err(error)
    }
    fn resolve(&mut self, _: Env, inner: Self::Output) -> Result<Self::JsValue> {
        Ok(Repository { inner })
    }
}
#[napi]
pub fn open(options: String) -> AsyncTask<Open> {
    AsyncTask::new(Open { options })
}
pub struct Call {
    inner: sdk::Repository,
    request: String,
    token: Option<String>,
}
impl Task for Call {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<String> {
        self.inner
            .call(&self.request, self.token.take())
            .map_err(error)
    }
    fn resolve(&mut self, _: Env, output: String) -> Result<String> {
        Ok(output)
    }
}
