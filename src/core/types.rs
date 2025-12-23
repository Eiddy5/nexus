use std::pin::Pin;

macro_rules! optional_async_method {
    ($name:ident,$payload:ty) => {
        fn $name(
            &self,
        ) -> Option<
            fn(
                $payload,
            ) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error>>> + Send>>,
        > {
            None
        }
    };
}

pub trait Extension: Send + Sync {
    // 优先级
    fn priority(&self) -> Option<u8>;
    // 扩展名称
    fn name(&self) -> Option<&str>;
    optional_async_method!(on_connection, OnConnectionPayload);
    optional_async_method!(on_create_document, OnCreateDocumentPayload);
    optional_async_method!(on_load_document, OnLoadDocumentPayload);
    optional_async_method!(after_load_document, AfterLoadDocumentPayload);
    optional_async_method!(on_change, OnChangePayload);
}

#[derive(Debug)]
pub struct OnCreateDocumentPayload {}

#[derive(Debug)]
pub struct OnConnectionPayload {}

pub struct OnLoadDocumentPayload {}

pub struct AfterLoadDocumentPayload {}

pub struct OnChangePayload {}
