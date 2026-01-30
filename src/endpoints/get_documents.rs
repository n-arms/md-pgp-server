use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use pgp::types::KeyId;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

use crate::signature::{key_id_from_text, key_id_to_text};

#[derive(Debug, Serialize, Deserialize)]
struct DocumentsInfo {
    name: String,
    last_updated: String,
}

#[derive(Deserialize)]
pub struct GetDocumentsParams {
    key_id: String,
}

async fn get_user_docs(
    pool: &SqlitePool,
    key_id: &KeyId,
) -> Result<HashMap<String, DocumentsInfo>, sqlx::Error> {
    let mut doc_ids = HashMap::new();
    let rows = sqlx::query(r#"select doc_id, name, last_updated from documents where user_id = ?"#)
        .bind(&key_id_to_text(key_id))
        .fetch_all(pool)
        .await?;

    for row in rows {
        let doc_id: String = row.get("doc_id");
        doc_ids.insert(
            doc_id,
            DocumentsInfo {
                name: row.get("name"),
                last_updated: row.get("last_updated"),
            },
        );
    }

    Ok(doc_ids)
}

pub async fn handle_get_documents(
    State(pool): State<SqlitePool>,
    Query(params): Query<GetDocumentsParams>,
) -> Result<String, (StatusCode, String)> {
    let key = match key_id_from_text(&params.key_id) {
        Ok(key) => key,
        Err(error) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Error getting documents:\n{error}"),
            ));
        }
    };
    match get_user_docs(&pool, &key).await {
        Ok(docs) => Ok(serde_json::to_string(&docs).unwrap()),
        Err(e) => {
            let error_message = e.to_string();
            Err((StatusCode::INTERNAL_SERVER_ERROR, error_message))
        }
    }
}
