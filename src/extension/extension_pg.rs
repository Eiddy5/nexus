use crate::core::types::{Extension, OnLoadDocumentPayload, OnStoreDocumentPayload};
use anyhow::Error;
use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{debug, error};
use yrs::updates::decoder::Decode;
use yrs::{Transact, Update};

pub struct PostgresExtension {
    pool: Arc<OnceCell<PgPool>>,
    host: String,
    port: String,
    username: String,
    password: String,
    database: String,
}

impl PostgresExtension {
    pub fn new(
        host: String,
        port: String,
        username: String,
        password: String,
        database: String,
    ) -> Self {
        Self {
            pool: Arc::new(OnceCell::new()),
            host,
            port,
            username,
            password,
            database,
        }
    }

    /// 获取数据库连接池（懒初始化）
    async fn get_pool(&self) -> Result<&PgPool, Error> {
        self.pool
            .get_or_try_init(|| async {
                let options = PgConnectOptions::new()
                    .host(&self.host)
                    .port(self.port.parse()?)
                    .username(&self.username)
                    .password(&self.password)
                    .database(&self.database);
                PgPoolOptions::new()
                    .max_connections(10)
                    .connect_with(options)
                    .await
                    .map_err(|e| anyhow::anyhow!("无法连接到 Postgres: {}", e))
            })
            .await
    }

    async fn fetch(&self, doc_id: &str) -> Result<Vec<u8>, Error> {
        let pool = match self.get_pool().await {
            Ok(p) => p,
            Err(e) => {
                error!("[PostgresExtension] 获取数据库连接失败: {}", e);
                return Err(e);
            }
        };

        let result = sqlx::query_scalar::<_, Option<Vec<u8>>>(
            "SELECT state_vector FROM public.doc_articles WHERE id = $1"
        )
        .bind(doc_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| anyhow::anyhow!("查询文档失败: {}", e))?;

        match result {
            Some(Some(state)) => {
                debug!(
                    "[PostgresExtension] 找到文档 {}, 大小: {} bytes",
                    doc_id,
                    state.len()
                );
                Ok(state)
            }
            Some(None) => {
                debug!(
                    "[PostgresExtension] 文档 {} 存在但 state_vector 为空",
                    doc_id
                );
                Ok(Vec::new())
            }
            None => {
                debug!("[PostgresExtension] 文档 {} 不存在，返回空状态", doc_id);
                Ok(Vec::new())
            }
        }
    }

        async fn store(&self, doc_id: &str, state: Vec<u8>) -> Result<(), Error> {
            let pool = match self.get_pool().await {
                Ok(p) => p,
                Err(e) => {
                    error!("[PostgresExtension] 获取数据库连接失败: {}", e);
                    return Err(e);
                }
            };

            // 插入或更新文档状态（使用 UPSERT）
            let result = sqlx::query(
                "INSERT INTO public.doc_articles (id, state_vector)
             VALUES ($1, $2)
             ON CONFLICT (id)
             DO UPDATE SET state_vector = $2",
            )
                .bind(doc_id)
                .bind(&state)
                .execute(pool)
                .await
                .map_err(|e| anyhow::anyhow!("存储文档失败: {}", e))?;

            debug!(
            "[PostgresExtension] 文档 {} 存储成功，影响行数: {}",
            doc_id,
            result.rows_affected()
        );
            Ok(())
        }
    }

    #[async_trait]
    impl Extension for PostgresExtension {
        fn priority(&self) -> Option<u8> {
            Some(10)
        }

        fn name(&self) -> Option<&str> {
            Some("postgres")
        }

        async fn on_store_document(
            &self,
            payload: OnStoreDocumentPayload,
        ) -> anyhow::Result<(), Error> {
            self.store(&payload.doc_id, payload.state).await?;
            Ok(())
        }

        async fn on_load_document(&self, payload: OnLoadDocumentPayload) -> Result<(), Error> {
            let state = self.fetch(&payload.document.doc_id).await?;

            // 如果数据库中没有数据，直接返回
            if state.is_empty() {
                debug!(
                "[PostgresExtension] 文档 {} 无历史数据",
                payload.document.doc_id
            );
                return Ok(());
            }
            let update = Update::decode_v1(&state)?;
            payload
                .document
                .awareness()
                .write()
                .await
                .doc_mut()
                .transact_mut()
                .apply_update(update)?;

            debug!(
            "[PostgresExtension] 文档 {} 加载成功",
            payload.document.doc_id
        );
            Ok(())
        }
    }
