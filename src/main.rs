mod flash;

use axum::{
    extract::{Extension, Form, Path, Query},
    http::StatusCode,
    response::Html,
    routing::{get, get_service, post},
    Router, Server,
};
use entity::{tags, transaction_tags, transactions, users};
use flash::{get_flash_cookie, post_response, PostResponse};
use migration::{Migrator, MigratorTrait};
use sea_orm::{prelude::*, Database, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::{env, net::SocketAddr};
use tags::Entity as Tags;
use tera::Tera;
use tower::ServiceBuilder;
use tower_cookies::{CookieManagerLayer, Cookies};
use tower_http::services::ServeDir;
use transaction_tags::Entity as TransactionTags;
use transactions::Entity as Transactions;
use users::Entity as Users;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env::set_var("RUST_LOG", "debug");
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
    let host = env::var("HOST").expect("HOST is not set in .env file");
    let port = env::var("PORT").expect("PORT is not set in .env file");
    let server_url = format!("{}:{}", host, port);

    let conn = Database::connect(db_url)
        .await
        .expect("Database connection failed");
    Migrator::up(&conn, None).await.unwrap();
    let templates = Tera::new(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"))
        .expect("Tera initialization failed");
    // let state = AppState { templates, conn };

    let app = Router::new()
        .route("/", get(list_transactions).post(create_transaction))
        .route("/:id", get(edit_transaction).post(update_transaction))
        .route("/new", get(new_transaction))
        .route("/delete/:id", post(delete_transaction))
        .nest(
            "/static",
            get_service(ServeDir::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/static"
            )))
            .handle_error(|error: std::io::Error| async move {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unhandled internal error: {}", error),
                )
            }),
        )
        .layer(
            ServiceBuilder::new()
                .layer(CookieManagerLayer::new())
                .layer(Extension(conn))
                .layer(Extension(templates)),
        );

    let addr = SocketAddr::from_str(&server_url).unwrap();
    Server::bind(&addr).serve(app.into_make_service()).await?;

    Ok(())
}

#[derive(Deserialize)]
struct Params {
    page: Option<usize>,
    transactions_per_page: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct FlashData {
    kind: String,
    message: String,
}

async fn list_transactions(
    Extension(ref templates): Extension<Tera>,
    Extension(ref conn): Extension<DatabaseConnection>,
    Query(params): Query<Params>,
    cookies: Cookies,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let page = params.page.unwrap_or(1);
    let transactions_per_page = params.transactions_per_page.unwrap_or(5);
    let paginator = Transactions::find()
        .order_by_asc(transactions::Column::Id)
        .paginate(conn, transactions_per_page);
    let num_pages = paginator.num_pages().await.ok().unwrap();
    let transacts = paginator
        .fetch_page(page - 1)
        .await
        .expect("could not retrieve transactions");

    let mut ctx = tera::Context::new();
    ctx.insert("transacts", &transacts);
    ctx.insert("page", &page);
    ctx.insert("transactions_per_page", &transactions_per_page);
    ctx.insert("num_pages", &num_pages);

    if let Some(value) = get_flash_cookie::<FlashData>(&cookies) {
        ctx.insert("flash", &value);
    }

    let body = templates
        .render("transaction_list.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn new_transaction(
    Extension(ref templates): Extension<Tera>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let ctx = tera::Context::new();
    let body = templates
        .render("new.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn create_transaction(
    Extension(ref conn): Extension<DatabaseConnection>,
    form: Form<transactions::Model>,
    mut cookies: Cookies,
) -> Result<PostResponse, (StatusCode, &'static str)> {
    let model = form.0;

    transactions::ActiveModel {
        date: Set(model.date.to_owned()),
        amount: Set(model.amount.to_owned()),
        expense: Set(model.expense.to_owned()),
        note: Set(model.note.to_owned()),
        user_id: Set(model.user_id.to_owned()),
        ..Default::default()
    }
    .save(conn)
    .await
    .expect("could not insert transaction");

    let data = FlashData {
        kind: "success".to_owned(),
        message: "Transaction successfully added".to_owned(),
    };

    Ok(post_response(&mut cookies, data))
}

async fn edit_transaction(
    Extension(ref templates): Extension<Tera>,
    Extension(ref conn): Extension<DatabaseConnection>,
    Path(id): Path<i32>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let transaction: transactions::Model = Transactions::find_by_id(id)
        .one(conn)
        .await
        .expect("could not find transaction")
        .unwrap();

    let mut ctx = tera::Context::new();
    ctx.insert("transaction", &transaction);

    let body = templates
        .render("edit.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn update_transaction(
    Extension(ref conn): Extension<DatabaseConnection>,
    Path(id): Path<i32>,
    form: Form<transactions::Model>,
    mut cookies: Cookies,
) -> Result<PostResponse, (StatusCode, &'static str)> {
    let model = form.0;

    transactions::ActiveModel {
        id: Set(id),
        date: Set(model.date.to_owned()),
        amount: Set(model.amount.to_owned()),
        expense: Set(model.expense.to_owned()),
        note: Set(model.note.to_owned()),
        user_id: Set(model.user_id.to_owned()),
    }
    .save(conn)
    .await
    .expect("could not edit post");

    let data = FlashData {
        kind: "success".to_owned(),
        message: "Post successfully updated".to_owned(),
    };

    Ok(post_response(&mut cookies, data))
}

async fn delete_transaction(
    Extension(ref conn): Extension<DatabaseConnection>,
    Path(id): Path<i32>,
    mut cookies: Cookies,
) -> Result<PostResponse, (StatusCode, &'static str)> {
    let transaction: transactions::ActiveModel = Transactions::find_by_id(id)
        .one(conn)
        .await
        .unwrap()
        .unwrap()
        .into();

    transaction.delete(conn).await.unwrap();

    let data = FlashData {
        kind: "success".to_owned(),
        message: "Transaction successfully deleted".to_owned(),
    };

    Ok(post_response(&mut cookies, data))
}
