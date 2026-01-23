use axum::{
    Router,
    body::{self},
    extract::State,
    http::StatusCode,
    routing::post,
};
use pgp::{
    composed::{Deserializable, SignedPublicKey},
    packet::Signature,
    ser::Serialize,
    types::{KeyDetails, KeyId},
};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::{fs::File, io};
use uuid::Uuid;

use crate::signature::{message_keyid, parse_message, verify_message};

mod signature;

#[tokio::main]
async fn main() {
    let pool = connect_db().await;
    // build our application with a single route
    let app = Router::new()
        .route("/create", post(handle_create_account))
        .with_state(pool.clone());

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("localhost:8000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn connect_db() -> SqlitePool {
    // write file if not exists
    let _file = File::create_new("data.db");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("file:data.db")
        .await
        .unwrap();

    // create tables if missing
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            uid TEXT PRIMARY KEY,
            key_blob BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS documents (
            doc_id TEXT PRIMARY KEY,
            name TEXT,
            user_id TEXT,
            shared_with TEXT,
            FOREIGN KEY (user_id) REFERENCES users(uid) 
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

fn parse_create_account(bytes: &[u8]) -> anyhow::Result<SignedPublicKey> {
    let (signature, plaintext) = parse_message(bytes)?;
    let key = SignedPublicKey::from_bytes(io::Cursor::new(plaintext.clone()))?;
    verify_message(&signature, &key, &plaintext)?;
    Ok(key)
}

fn key_id_to_text(key_id: &KeyId) -> String {
    hex::encode(key_id.as_ref())
}

async fn handle_create_account(
    State(pool): State<SqlitePool>,
    body: body::Bytes,
) -> Result<String, (StatusCode, String)> {
    let key = match parse_create_account(&body) {
        Ok(key) => key,
        Err(error) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Bad create account:\n{error}"),
            ));
        }
    };
    match insert_user(&pool, &key).await {
        Ok(()) => Ok(format!("ok")),
        Err(e) => {
            let error_message = e.to_string();
            if error_message.contains("UNIQUE constraint failed") {
                Err((StatusCode::CONFLICT, "user already exists".to_string()))
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, error_message))
            }
        }
    }
}

async fn insert_user(pool: &SqlitePool, key: &SignedPublicKey) -> anyhow::Result<()> {
    let key_id = key.key_id();
    let key_blob = key.to_bytes()?;
    sqlx::query(r#"insert into users (uid, key_blob) values (?, ?)"#)
        .bind(key_id_to_text(&key_id))
        .bind(key_blob)
        .execute(pool)
        .await?;
    Ok(())
}

async fn create_document(pool: &SqlitePool, owner_key_id: &String, doc_name: &String) -> Uuid {
    let id = Uuid::now_v7();

    sqlx::query(r#"insert into documents (doc_id, name, user_id) values (?, ?, ?)"#)
        .bind(&id.to_string())
        .bind(&doc_name)
        .bind(&owner_key_id)
        .execute(pool)
        .await
        .unwrap();

    id
}

async fn share_document(
    pool: &SqlitePool,
    doc_id: &Uuid,
    owner_key_id: &String,
    user_key_id: &String,
) {
    // get document from id
    // check owner
    let doc_row = sqlx::query(r#"select user_id from documents where doc_id = ?"#)
        .bind(&doc_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
    let owner_id: String = doc_row.get("user_id");
    if owner_id != *owner_key_id {
        panic!("not owner");
    }
    // check new user in users table
    let users_row = sqlx::query(r#"select uid from users where uid = ?"#)
        .bind(&user_key_id)
        .fetch_one(pool)
        .await
        .unwrap();

    let users = users_row.get::<String, _>("uid");
    if users != *user_key_id {
        panic!("user does not exist");
    }

    // parse shared ids to vec
    let mut shared_ids = [].to_vec();
    let shared_row = sqlx::query(r#"select shared_with from documents where doc_id = ?"#)
        .bind(&doc_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
    let shared_with: String = shared_row.get("shared_with");
    if shared_with.len() > 0 {
        for id in shared_with.split(",") {
            shared_ids.push(id.to_string());
        }
    }

    // add to vec
    shared_ids.push(user_key_id.to_string());

    // iter fold back to string
    let shared_with_str = shared_ids.iter().fold(String::new(), |acc, x| {
        if acc.len() == 0 {
            x.to_string()
        } else {
            format!("{},{}", acc, x)
        }
    });

    // update document
    sqlx::query(r#"update documents set shared_with = ? where doc_id = ?"#)
        .bind(&shared_with_str)
        .bind(&doc_id.to_string())
        .execute(pool)
        .await
        .unwrap();
}

async fn get_user_docs(pool: &SqlitePool, key_id: &String) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut doc_ids = [].to_vec();
    let rows = sqlx::query(r#"select doc_id from documents where user_id = ?"#)
        .bind(&key_id)
        .fetch_all(pool)
        .await?;

    for row in rows {
        let doc_id: String = row.get("doc_id");
        doc_ids.push(Uuid::parse_str(&doc_id).unwrap());
    }

    Ok(doc_ids)
}
