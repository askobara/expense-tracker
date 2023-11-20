use anyhow::Result;
use dotenvy::dotenv;
use std::collections::HashMap;
use std::env;
use std::str::FromStr;

trait CustomProperties {
    fn category_db(&self) -> Option<Vec<notion::ids::PageId>>;
}

impl CustomProperties for notion::models::Properties
{
    fn category_db(&self) -> Option<Vec<notion::ids::PageId>> {
        self.properties.values().find_map(|p| match p {
            notion::models::properties::PropertyValue::Relation { relation, .. } => {
                Some(relation.as_ref().unwrap_or(&vec![]).iter().map(|t| t.id.clone()).collect())
            }
            _ => None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().expect(".env file not found");

    let notion_api = notion::NotionApi::new(env::var("NOTION_API_KEY")?)?;
    let db_id = notion::ids::DatabaseId::from_str(&env::var("NOTION_DATABASE_ID")?)?;
    let db: notion::models::Database = notion_api.get_database(db_id).await?;

    // let result = notion_api.query_database(
    //     &db.id,
    //     notion::models::search::DatabaseQuery::default()
    // ).await?;
    //
    // dbg!(result);
    //
    // return Ok(());

    let mut properties: HashMap<String, notion::models::properties::PropertyValue> = HashMap::new();

    match db.properties.get("Name") {
        Some(notion::models::properties::PropertyConfiguration::Title { id }) => {

            let title = vec![
                notion::models::text::RichText::Text {
                    rich_text: notion::models::text::RichTextCommon { plain_text: "test".to_string(), href: None, annotations: None },
                    text: notion::models::text::Text { content: "test".to_string(), link: None }
                }
            ];

            properties.insert(
                "Name".to_string(),
                notion::models::properties::PropertyValue::Title { id: id.clone(), title }
            );
        }
        _ => {}
    };

    match db.properties.get("Amount") {
        Some(notion::models::properties::PropertyConfiguration::Number { id, .. }) => {
            properties.insert(
                "Amount".to_string(),
                notion::models::properties::PropertyValue::Number { id: id.clone(), number: Some(serde_json::Number::from(100) ) }
            );
        }
        _ => {}
    };

    match db.properties.get("Date") {
        Some(notion::models::properties::PropertyConfiguration::Date { id }) => {

            let now = notion::chrono::offset::Local::now();

            properties.insert(
                "Date".to_string(),
                notion::models::properties::PropertyValue::Date {
                    id: id.clone(),
                    date: Some(notion::models::properties::DateValue {
                        start: notion::models::properties::DateOrDateTime::Date(now.date_naive()),
                        end: None,
                        time_zone: None
                    })
                }
            );
        }
        _ => {}
    };

    match db.properties.get("Category") {
        Some(notion::models::properties::PropertyConfiguration::Relation { id, relation }) => {
            let result = notion_api.query_database(
                &relation.database_id,
                notion::models::search::DatabaseQuery::default()
            ).await?;

            let f = result.results.first().unwrap(); // TODO: add skim

            let r = vec![notion::models::properties::RelationValue { id: f.id.clone() }];

            properties.insert(
                "Category".to_string(),
                notion::models::properties::PropertyValue::Relation { id: id.clone(), relation: Some(r) }
            );
        }
        _ => {}
    };

    // dbg!(properties);

    let request = notion::models::PageCreateRequest {
        parent: notion::models::Parent::Database { database_id: db.id },
        properties: notion::models::Properties { properties }
    };

    let response = notion_api.create_page(request).await?;

    dbg!(response);

    Ok(())
}
