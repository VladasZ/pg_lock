use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct Item {
    #[allow(dead_code)]
    id:       i32,
    pub name: String,
}
