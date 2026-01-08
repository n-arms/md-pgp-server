use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use sqlx::{SqlitePool, pool, sqlite::SqlitePoolOptions};

#[tokio::main]
async fn main() {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("file:data.db")
        .await
        .unwrap();
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

async fn handle_get_key(State(pool): State<SqlitePool>, Path(key): Path<String>) -> String {
    insert_user(pool, &key).await;

    format!("Got key {key:?}")
}

async fn insert_user(pool: SqlitePool, fingerprint: &String) {
    let result = sqlx::query(r#"insert into users (uid) values (?)"#)
        .bind(&fingerprint)
        .execute(&pool)
        .await;
    if result.is_err() {
        println!("Error inserting user: {:?}", result.err());
    }
}
