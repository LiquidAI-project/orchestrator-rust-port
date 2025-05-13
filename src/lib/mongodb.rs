use mongodb::{Client, options::ClientOptions};
use std::env;
use mongodb::error::Result;
use dotenv::dotenv;

pub async fn initialize_client() -> Result<Client> {
    dotenv().ok();

    let host = env::var("MONGO_HOST").expect("MONGO_HOST not set");
    let port = env::var("MONGO_PORT").expect("MONGO_PORT not set");
    let user = env::var("MONGO_ROOT_USERNAME").expect("MONGO_ROOT_USERNAME not set");
    let pass = env::var("MONGO_ROOT_PASSWORD").expect("MONGO_ROOT_PASSWORD not set");

    let uri = format!("mongodb://{}:{}@{}:{}/", user, pass, host, port);
    let options = ClientOptions::parse(&uri).await?;

    Ok(Client::with_options(options)?)
}
