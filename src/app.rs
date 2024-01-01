use eyre::Result;
use std::collections::HashMap;

pub struct App {
    settings: crate::settings::Settings,
    notion_api: notion::NotionApi,
    categories_cache: Option<Vec<notion::models::Page>>,
}

fn select_page(
    pages: &Vec<notion::models::Page>,
    preselect: Option<String>,
) -> Result<notion::ids::PageId> {
    struct Page<'a> {
        page: &'a notion::models::Page,
    }

    impl<'a> std::fmt::Display for Page<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.page.title().unwrap_or("Untitled".to_string()))
        }
    }

    let options: Vec<Page> = pages.into_iter().map(|page| Page { page }).collect();
    let pos = preselect.and_then(|ps| options.iter().position(|p| p.to_string() == ps));

    let mut select = inquire::Select::new("Category:", options);
    if let Some(pos) = pos {
        select = select.with_starting_cursor(pos);
    }

    let result = select.prompt()?;

    Ok(result.page.id.clone())
}

impl App {
    pub fn new() -> Result<Self> {
        let settings = crate::settings::Settings::new()?;
        let notion_api = notion::NotionApi::new(settings.notion.api_key.clone())?;

        Ok(Self {
            settings,
            notion_api,
            categories_cache: None,
        })
    }

    pub async fn run() -> Result<()> {
        let mut app = Self::new()?;

        let db: notion::models::Database = app
            .notion_api
            .get_database(&app.settings.notion.database_id)
            .await?;

        let confirm = inquire::Confirm::new("Want to add one more row?").with_default(false);

        loop {
            app.create_page(&db).await?;

            match confirm.clone().prompt() {
                Ok(true) => continue,
                _ => {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn create_page(&mut self, db: &notion::models::Database) -> Result<notion::models::Page> {
        let properties = self.create_page_properties(&db.properties).await?;

        let request = notion::models::PageCreateRequest {
            parent: notion::models::Parent::Database {
                database_id: db.id.clone(),
            },
            properties: notion::models::Properties { properties },
        };

        self.notion_api
            .create_page(request)
            .await
            .map_err(eyre::Error::new)
    }

    async fn get_database_pages(
        &self,
        database_id: &notion::ids::DatabaseId,
    ) -> Result<Vec<notion::models::Page>> {
        let result = self
            .notion_api
            .query_database(
                database_id,
                notion::models::search::DatabaseQuery::default(),
            )
            .await?;

        Ok(result.results)
    }

    async fn create_page_properties(
        &mut self,
        db_properties: &HashMap<String, notion::models::properties::PropertyConfiguration>,
    ) -> Result<HashMap<String, notion::models::properties::PropertyValue>> {
        let mut properties: HashMap<String, notion::models::properties::PropertyValue> =
            HashMap::new();

        let mut preselect = None;

        if let Some(notion::models::properties::PropertyConfiguration::Title { id }) =
            db_properties.get("Name")
        {
            let name = inquire::Text::new("Name:").prompt()?;

            let title = vec![notion::models::text::RichText::Text {
                rich_text: notion::models::text::RichTextCommon {
                    plain_text: name.clone(),
                    href: None,
                    annotations: None,
                },
                text: notion::models::text::Text {
                    content: name.clone(),
                    link: None,
                },
            }];

            preselect = match name.as_ref() {
                "Самокат" => Some("Food".to_string()),
                "Самокат чаевые" => Some("Food".to_string()),
                "Метро" => Some("Transport".to_string()),
                "Такси" => Some("Transport".to_string()),
                "Такси чаевые" => Some("Transport".to_string()),
                "Тренер" => Some("Fitness".to_string()),
                "Квартира" => Some("Bills & Utilities".to_string()),
                "Новотелеком" => Some("Bills & Utilities".to_string()),
                "Yota" => Some("Bills & Utilities".to_string()),
                "Квартплата" => Some("Bills & Utilities".to_string()),
                "Колорлон" => Some("DIY".to_string()),
                _ => None,
            };

            properties.insert(
                "Name".to_string(),
                notion::models::properties::PropertyValue::Title {
                    id: id.clone(),
                    title,
                },
            );
        }

        if let Some(notion::models::properties::PropertyConfiguration::Number { id, .. }) =
            db_properties.get("Amount")
        {
            let amount = inquire::Text::new("Amount:").prompt()?;

            properties.insert(
                "Amount".to_string(),
                notion::models::properties::PropertyValue::Number {
                    id: id.clone(),
                    number: serde_json::Number::from_f64(amount.parse::<f64>()?),
                },
            );
        }

        if let Some(notion::models::properties::PropertyConfiguration::Date { id }) =
            db_properties.get("Date")
        {
            let now = notion::chrono::offset::Local::now();
            let date = inquire::DateSelect::new("Date:")
                .with_default(now.date_naive())
                .with_min_date(
                    now.checked_sub_days(notion::chrono::Days::new(7))
                        .unwrap()
                        .date_naive(),
                )
                .with_max_date(now.date_naive())
                .with_week_start(notion::chrono::Weekday::Mon)
                .prompt()?;

            properties.insert(
                "Date".to_string(),
                notion::models::properties::PropertyValue::Date {
                    id: id.clone(),
                    date: Some(notion::models::properties::DateValue {
                        start: notion::models::properties::DateOrDateTime::Date(date),
                        end: None,
                        time_zone: None,
                    }),
                },
            );
        }

        if let Some(notion::models::properties::PropertyConfiguration::Relation { id, relation }) =
            db_properties.get("Category")
        {
            if self.categories_cache.is_none() {
                self.categories_cache = self.get_database_pages(&relation.database_id).await.ok();
            }

            if let Some(pages) = &self.categories_cache {
                let page_id = select_page(&pages, preselect)?;

                properties.insert(
                    "Category".to_string(),
                    notion::models::properties::PropertyValue::Relation {
                        id: id.clone(),
                        relation: Some(vec![notion::models::properties::RelationValue {
                            id: page_id,
                        }]),
                    },
                );
            }
        }

        Ok(properties)
    }
}
