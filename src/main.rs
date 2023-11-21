use anyhow::Result;
use dotenvy::dotenv;
use std::collections::HashMap;
use std::env;
use std::str::FromStr;

fn select_page(pages: Vec<notion::models::Page>) -> Result<notion::ids::PageId> {
    struct Page {
        page: notion::models::Page,
    }

    impl std::fmt::Display for Page {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.page.title().unwrap_or("Untitled".to_string()))
        }
    }

    let options = pages.into_iter().map(|page| Page { page }).collect();
    let result = inquire::Select::new("Category:", options).prompt()?;

    Ok(result.page.id.clone())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().expect(".env file not found");

    let notion_api = notion::NotionApi::new(env::var("NOTION_API_KEY")?)?;
    let db_id = notion::ids::DatabaseId::from_str(&env::var("NOTION_DATABASE_ID")?)?;
    let db: notion::models::Database = notion_api.get_database(db_id).await?;

    let mut properties: HashMap<String, notion::models::properties::PropertyValue> = HashMap::new();

    match db.properties.get("Name") {
        Some(notion::models::properties::PropertyConfiguration::Title { id }) => {
            let name = inquire::Text::new("Name:").prompt()?;

            let title = vec![
                notion::models::text::RichText::Text {
                    rich_text: notion::models::text::RichTextCommon { plain_text: name.clone(), href: None, annotations: None },
                    text: notion::models::text::Text { content: name, link: None }
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
            let amount = inquire::Text::new("Amount:").prompt()?;

            properties.insert(
                "Amount".to_string(),
                notion::models::properties::PropertyValue::Number { id: id.clone(), number: serde_json::Number::from_f64(amount.parse::<f64>()?) }
            );
        }
        _ => {}
    };

    match db.properties.get("Date") {
        Some(notion::models::properties::PropertyConfiguration::Date { id }) => {

            let now = notion::chrono::offset::Local::now();
            let date = inquire::DateSelect::new("Date:")
                .with_default(now.date_naive())
                .with_min_date(now.checked_sub_days(notion::chrono::Days::new(7)).unwrap().date_naive())
                .with_max_date(now.date_naive())
                .with_week_start(notion::chrono::Weekday::Mon)
                // .with_help_message("Possible flights will be displayed according to the selected date")
                .prompt()?;

            properties.insert(
                "Date".to_string(),
                notion::models::properties::PropertyValue::Date {
                    id: id.clone(),
                    date: Some(notion::models::properties::DateValue {
                        start: notion::models::properties::DateOrDateTime::Date(date),
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

            let page_id = select_page(result.results)?;
            let r = vec![notion::models::properties::RelationValue { id: page_id }];

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
