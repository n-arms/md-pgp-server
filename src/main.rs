use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::fs::File;
use uuid::Uuid;

#[tokio::main]

async fn main() {
    let pool = connect_db().await;
    // build our application with a single route
    let app = Router::new()
        .route("/key/{key}", get(handle_get_key))
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
            uid TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS documents (
            doc_id TEXT PRIMARY KEY,
            name TEXT,
            user_id TEXT,
            FOREIGN KEY (user_id) REFERENCES users(uid) 
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

async fn handle_get_key(
    State(pool): State<SqlitePool>,
    Path(key): Path<String>,
) -> Result<String, (StatusCode, String)> {
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

async fn insert_user(pool: &SqlitePool, fingerprint: &String) -> Result<(), sqlx::Error> {
    sqlx::query(r#"insert into users (uid) values (?)"#)
        .bind(&fingerprint)
        .execute(pool)
        .await?;
    Ok(())
}

async fn create_document(pool: &SqlitePool, owner_fingerprint: &String, doc_name: &String) -> Uuid {
    let id = Uuid::now_v7();

    sqlx::query(r#"insert into documents (doc_id, name, user_id) values (?, ?, ?)"#)
        .bind(&id.to_string())
        .bind(&doc_name)
        .bind(&owner_fingerprint)
        .execute(pool)
        .await
        .unwrap();

    id
}

async fn share_document(
    pool: &SqlitePool,
    doc_id: &Uuid,
    owner_fingerprint: &String,
    user_fingerprint: &String,
) {
    // get document from id
    // check owner
    let doc_row = sqlx::query(r#"select user_id from documents where doc_id = ?"#)
        .bind(&doc_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
    let owner_id: String = doc_row.get("user_id");
    if owner_id != *owner_fingerprint {
        panic!("not owner");
    }
    // check new user in users table
    let users_row = sqlx::query(r#"select uid from users where uid = ?"#)
        .bind(&user_fingerprint)
        .fetch_one(pool)
        .await
        .unwrap();

    let users = users_row.get::<String, _>("uid");
    if users != *user_fingerprint {
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
    shared_ids.push(user_fingerprint.to_string());

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

async fn get_user_docs(pool: &SqlitePool, fingerprint: &String) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut doc_ids = [].to_vec();
    let rows = sqlx::query(r#"select doc_id from documents where user_id = ?"#)
        .bind(&fingerprint)
        .fetch_all(pool)
        .await?;

    for row in rows {
        let doc_id: String = row.get("doc_id");
        doc_ids.push(Uuid::parse_str(&doc_id).unwrap());
    }

    Ok(doc_ids)
}
