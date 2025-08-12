use std::env;
use mongodb::{Client, Collection, bson::Document};
use mongodb::options::ClientOptions;
use mongodb::bson::{doc, Bson};
use serde::{Serialize, de::DeserializeOwned};

/// Connect to MongoDB and return a typed collection by name.
pub async fn get_collection<T: DeserializeOwned + Unpin + Send + Sync>(
    collection_name: &str,
) -> Collection<T> {
    let host = env::var("MONGO_HOST").unwrap_or_else(|_| "localhost".into());
    let port = env::var("MONGO_PORT").unwrap_or_else(|_| "27017".into());
    let user = env::var("MONGO_ROOT_USERNAME").unwrap_or_else(|_| "root".into());
    let pass = env::var("MONGO_ROOT_PASSWORD").unwrap_or_else(|_| "example".into());

    let uri = format!("mongodb://{}:{}@{}:{}/?authSource=admin", user, pass, host, port);
    let options = ClientOptions::parse(&uri).await.expect("Invalid MongoDB URI");
    let client = Client::with_options(options).expect("MongoDB client init failed");

    client.database("wasmiot").collection::<T>(collection_name)
}

/// Find a single document in the given collection using a BSON query.
pub async fn find_one<T: DeserializeOwned + Unpin + Send + Sync>(
    collection_name: &str,
    query: Document,
) -> mongodb::error::Result<Option<T>> {
    let collection = get_collection::<T>(collection_name).await;
    collection.find_one(query).await
}

/// Insert a document into the given collection.
pub async fn insert_one<T: Serialize + DeserializeOwned + Unpin + Send + Sync>(
    collection_name: &str,
    document: &T,
) -> mongodb::error::Result<Bson> {
    let collection = get_collection::<T>(collection_name).await;
    let result = collection.insert_one(document).await?;
    Ok(result.inserted_id)
}

/// Update a single BSON field on a document matching the query.
pub async fn update_field<T: Serialize + DeserializeOwned + Unpin + Send + Sync>(
    collection_name: &str,
    query: Document,
    field: &str,
    value: Bson,
) -> mongodb::error::Result<()> {
    let collection = get_collection::<T>(collection_name).await;
    let update_doc = doc! { "$set": { field: value } };
    collection.update_one(query, update_doc).await.map(|_| ())
}
