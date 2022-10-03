#[macro_use]
extern crate argon2;

mod flash;

use argon2::Config;
use axum::{
    extract::{Extension, Form, Path, Query},
    http::{Error, StatusCode},
    response::Html,
    routing::{get, get_service, post},
    Router, Server,
};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use entity::{tags, transaction_tags, transactions, users};
use flash::{get_flash_cookie, post_response, PostResponse};
use migration::{Condition, Migrator, MigratorTrait};
use sea_orm::{prelude::*, Database, FromQueryResult, QueryOrder, QuerySelect, Set};
use sea_query::Expr;
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr};
use std::{iter::Sum, str::FromStr};
use tags::Entity as Tags;
use tera::Tera;
use tower::ServiceBuilder;
use tower_cookies::{CookieManagerLayer, Cookies};
use tower_http::services::ServeDir;
use transaction_tags::Entity as TransactionTags;
use transactions::Entity as Transactions;
use users::Entity as Users;

pub const USER_ID_FOR_TEST: i32 = 1;

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
        .route("/", get(total_transactions).post(create_transaction))
        .route("/:id", get(edit_transaction).post(update_transaction))
        .route("/new", get(new_transaction))
        .route("/delete/:id", post(delete_transaction))
        .route("/list", get(list_transactions))
        .route("/login", get(login_page).post(sign_in))
        .route("/signup", get(signup_page).post(sign_up))
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
    let user_1 = UserParams {
        user_id: 1,
        todays_date: Local::now().naive_local().date(),
        next_income_date: Local::now().naive_local().date() + Duration::days(1),
    };
    let expense_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(true)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let expense_sum = expense_transaction.amount;

    let income_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(false)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let income_sum = income_transaction.amount;

    let total = income_sum - expense_sum;

    let total_fmt = format!("$ {}", total.round_dp(2));

    let mut ctx = tera::Context::new();
    ctx.insert("user_id", &user_1.user_id);
    ctx.insert("today", &user_1.todays_date);
    ctx.insert("next_income_date", &user_1.next_income_date);
    ctx.insert("sum", &total_fmt);

    let page = params.page.unwrap_or(1);
    let transactions_per_page = params.transactions_per_page.unwrap_or(10);
    let paginator = Transactions::find()
        .order_by_asc(transactions::Column::Date)
        .paginate(conn, transactions_per_page);
    let num_pages = paginator.num_pages().await.ok().unwrap();
    let transacts = paginator
        .fetch_page(page - 1)
        .await
        .expect("could not retrieve transactions");

    //let mut ctx = tera::Context::new();
    ctx.insert("transacts", &transacts);
    ctx.insert("page", &page);
    ctx.insert("transactions_per_page", &transactions_per_page);
    ctx.insert("num_pages", &num_pages);

    //if let Some(value) = get_flash_cookie::<FlashData>(&cookies) {
    //    ctx.insert("flash", &value);
    //}

    let body = templates
        .render("index.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn new_transaction(
    Extension(ref templates): Extension<Tera>,
    Extension(ref conn): Extension<DatabaseConnection>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let user_1 = UserParams {
        user_id: 1,
        todays_date: Local::now().naive_local().date(),
        next_income_date: Local::now().naive_local().date() + Duration::days(1),
    };
    let expense_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(true)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let expense_sum = expense_transaction.amount;

    let income_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(false)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let income_sum = income_transaction.amount;

    let total = income_sum - expense_sum;

    let total_fmt = format!("$ {}", total.round_dp(2));

    let mut ctx = tera::Context::new();
    ctx.insert("user_id", &user_1.user_id);
    ctx.insert("today", &user_1.todays_date);
    ctx.insert("next_income_date", &user_1.next_income_date);
    ctx.insert("sum", &total_fmt);
    //let ctx = tera::Context::new();
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
    let user_1 = UserParams {
        user_id: 1,
        todays_date: Local::now().naive_local().date(),
        next_income_date: Local::now().naive_local().date() + Duration::days(1),
    };
    let expense_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(true)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let expense_sum = expense_transaction.amount;

    let income_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1.next_income_date))
                .add(transactions::Column::Expense.eq(false)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let income_sum = income_transaction.amount;

    let total = income_sum - expense_sum;

    let total_fmt = format!("$ {}", total.round_dp(2));

    let mut ctx = tera::Context::new();
    ctx.insert("user_id", &user_1.user_id);
    ctx.insert("today", &user_1.todays_date);
    ctx.insert("next_income_date", &user_1.next_income_date);
    ctx.insert("sum", &total_fmt);

    let transaction: transactions::Model = Transactions::find_by_id(id)
        .one(conn)
        .await
        .expect("could not find transaction")
        .unwrap();

    //let mut ctx = tera::Context::new();
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
    .expect("could not edit transaction");

    let data = FlashData {
        kind: "success".to_owned(),
        message: "Transaction successfully updated".to_owned(),
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

#[derive(Deserialize)]
struct UserParams {
    user_id: i32,
    todays_date: Date,
    next_income_date: Date,
}

#[derive(Deserialize, FromQueryResult)]
struct SumResult {
    amount: Decimal,
}

async fn total_transactions(
    Extension(ref templates): Extension<Tera>,
    Extension(ref conn): Extension<DatabaseConnection>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let user_1 = UserParams {
        user_id: 1,
        todays_date: Local::now().naive_local().date(),
        next_income_date: Local::now().naive_local().date() + Duration::days(1),
    };
    let next_income: transactions::ActiveModel = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.gt(user_1.todays_date))
                .add(transactions::Column::Expense.eq(false)),
        )
        .order_by_asc(transactions::Column::Date)
        .one(conn)
        .await
        .unwrap()
        .unwrap()
        .into();

    let user_1_update = UserParams {
        user_id: 1,
        todays_date: Local::now().naive_local().date(),
        next_income_date: next_income.date.unwrap(),
    };
    let expense_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1_update.next_income_date))
                .add(transactions::Column::Expense.eq(true)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let expense_sum = expense_transaction.amount;

    let income_transaction: SumResult = Transactions::find()
        .filter(
            Condition::all()
                .add(transactions::Column::Date.lt(user_1_update.next_income_date))
                .add(transactions::Column::Expense.eq(false)),
        )
        .select_only()
        .column_as(Expr::col(transactions::Column::Amount).sum(), "amount")
        .into_model::<SumResult>()
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let income_sum = income_transaction.amount;

    let total = income_sum - expense_sum;

    let total_fmt = format!("$ {}", total.round_dp(2));

    let mut ctx = tera::Context::new();
    ctx.insert("user_id", &user_1.user_id);
    ctx.insert("today", &user_1.todays_date);
    ctx.insert("next_income_date", &user_1_update.next_income_date);
    ctx.insert("sum", &total_fmt);

    let body = templates
        .render("total.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn login_page(
    Extension(ref templates): Extension<Tera>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let ctx = tera::Context::new();
    let body = templates
        .render("login.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn sign_in(
    Extension(ref conn): Extension<DatabaseConnection>,
    form: Form<users::Model>,
    mut cookies: Cookies,
) -> Result<PostResponse, (StatusCode, &'static str)> {
    let model = form.0;

    let stored_user = Users::find()
        .filter(users::Column::Username.contains(&model.username))
        .one(conn)
        .await
        .expect("User id not found")
        .unwrap();

    //let cloned_pass = model.password.clone();
    let password_hash = hash_password(model.password).await.unwrap();

    verify_password(stored_user.password, password_hash)
        .await
        .unwrap();

    let data = FlashData {
        kind: "success".to_owned(),
        message: "User successfully verified".to_owned(),
    };

    Ok(post_response(&mut cookies, data))
}

async fn signup_page(
    Extension(ref templates): Extension<Tera>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let ctx = tera::Context::new();
    let body = templates
        .render("signup.html.tera", &ctx)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error"))?;

    Ok(Html(body))
}

async fn sign_up(
    Extension(ref conn): Extension<DatabaseConnection>,
    form: Form<users::Model>,
    mut cookies: Cookies,
) -> Result<PostResponse, (StatusCode, &'static str)> {
    let model = form.0;
    let hashed_pass = hash_password(model.password).await.unwrap();

    users::ActiveModel {
        username: Set(model.username.to_owned()),
        password: Set(hashed_pass.to_owned()),
        ..Default::default()
    }
    .save(conn)
    .await
    .expect("could not create user");

    let data = FlashData {
        kind: "success".to_owned(),
        message: "Transaction successfully updated".to_owned(),
    };

    Ok(post_response(&mut cookies, data))
}
async fn hash_password(password: String) -> Result<String, anyhow::Error> {
    // Argon2 hashing is designed to be computationally intensive,
    // so we need to do this on a blocking thread.
    dotenv::dotenv().ok();
    let salt_string = env::var("SALT").unwrap();
    //let salt: &[u8] = salt_string.as_bytes();
    let salt = salt_string.as_bytes();
    let config = Config::default();
    let pass_form = format!("{}", password);
    let pwd = pass_form.as_bytes();
    let hashed =
        argon2::hash_encoded(pwd, salt, &config).expect("failed to generate password hash");
    Ok(hashed)
}

async fn verify_password(password: String, password_hash: String) -> Result<(), anyhow::Error> {
    let pwd = password.as_bytes();
    argon2::verify_encoded(&password_hash, pwd).unwrap();
    Ok(())
}
