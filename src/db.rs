use diesel_async::{pooled_connection::AsyncDieselConnectionManager, AsyncPgConnection};

pub type Pool = bb8::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>;
