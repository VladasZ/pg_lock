mod model;

use anyhow::Result;
use sqlx::{
    Error, PgPool,
    postgres::{PgPoolOptions, PgQueryResult},
    query_as,
};
use tokio::{
    task::JoinHandle,
    time::{Duration, sleep},
};

use crate::model::Item;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    dbg!(&database_url);

    let pool = PgPoolOptions::new().max_connections(5).connect(&database_url).await?;

    prepare_data(&pool).await?;

    let data: Vec<Item> = query_as("SELECT * FROM items").fetch_all(&pool).await?;

    // Both object can be queried
    assert_eq!(
        data.iter().map(|i| &i.name).collect::<Vec<_>>(),
        vec!["item1", "item2"]
    );

    dbg!(&data);

    let pool1 = pool.clone();
    let pool2 = pool.clone();

    let tx1 = spawn_transaction(pool1, 1, 2);
    let tx2 = spawn_transaction(pool2, 2, 1);

    let (pool1, pool2) = tokio::try_join!(tx1, tx2)?;

    let data: Vec<Item> = query_as("SELECT * FROM items").fetch_all(&pool).await?;
    dbg!(&data);

    let data1: Vec<Item> = query_as("SELECT * FROM items").fetch_all(&pool1).await?;
    dbg!(&data1);

    let data2: Vec<Item> = query_as("SELECT * FROM items").fetch_all(&pool2).await?;
    dbg!(&data2);

    Ok(())
}

async fn prepare_data(pool: &PgPool) -> Result<()> {
    sqlx::query("DROP TABLE items").execute(pool).await?;

    sqlx::query(
        r#"
    CREATE TABLE IF NOT EXISTS items (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL
    );
    "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("INSERT INTO items (name) VALUES ($1), ($2)")
        .bind("item1")
        .bind("item2")
        .execute(pool)
        .await?;

    Ok(())
}

fn spawn_transaction(pool: PgPool, id_1: i32, id_2: i32) -> JoinHandle<PgPool> {
    tokio::spawn(async move { perform_transaction(pool, id_1, id_2).await.unwrap() })
}

async fn perform_transaction(pool: PgPool, id_1: i32, id_2: i32) -> Result<PgPool> {
    let mut tx = pool.begin().await?;

    sqlx::query(&format!("SELECT * FROM items WHERE id = {id_1} FOR UPDATE"))
        .execute(tx.as_mut())
        .await?;

    sleep(Duration::from_millis(100)).await;

    let result = sqlx::query(&format!("SELECT * FROM items WHERE id = {id_2} FOR UPDATE"))
        .execute(tx.as_mut())
        .await;

    if check_for_deadlock_error(&result) {
        println!("Deadlock detected. Returning broken pool");
        dbg!(&result);
        return Ok(pool);
    }

    result?;

    tx.commit().await?;

    Ok(pool)
}

fn check_for_deadlock_error(result: &Result<PgQueryResult, Error>) -> bool {
    let Err(err) = result else { return false };

    let Error::Database(db_error) = err else {
        return false;
    };

    db_error.message() == "deadlock detected"
}
