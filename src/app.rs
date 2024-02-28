use eyre::Result;
use std::collections::HashMap;

pub struct App {
    settings: crate::settings::Settings,
    notion_api: notion::NotionApi,
    categories_cache: Option<Vec<notion::models::Page>>,
    last_date: Option<notion::chrono::NaiveDate>,
}

fn select_page(
    pages: &Vec<notion::models::Page>,
    preselect: Option<&String>,
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
    let pos = preselect.and_then(|ps| options.iter().position(|p| p.to_string() == ps.as_str()));

    let mut select = inquire::Select::new("Category:", options);
    if let Some(pos) = pos {
        select = select.with_starting_cursor(pos);
    }

    let result = select.prompt()?;

    Ok(result.page.id.clone())
}

fn page_property_to_string(page: &notion::models::Page, name: &str) -> Option<String> {
    match page.properties.properties.get(name) {
        Some(notion::models::properties::PropertyValue::Date { id: _, date }) => match date {
            Some(date) => match date.start {
                notion::models::properties::DateOrDateTime::Date(date) => Some(date.to_string()),
                _ => None,
            },
            _ => None,
        },
        Some(notion::models::properties::PropertyValue::Number { id: _, number }) => {
            number.clone().map(|v| v.to_string())
        }
        Some(_) => todo!(),
        None => None,
    }
}

fn database_sorting(property: impl Into<String>, page_size: u8) -> notion::models::search::DatabaseQuery {
    notion::models::search::DatabaseQuery {
        sorts: Some(vec![notion::models::search::DatabaseSort {
            property: Some(property.into()),
            timestamp: None,
            direction: notion::models::search::SortDirection::Descending,
        }]),
        paging: Some(notion::models::paging::Paging {
            start_cursor: None,
            page_size: Some(page_size),
        }),
        filter: None,
    }
}

impl App {
    pub fn new() -> Result<Self> {
        let settings = crate::settings::Settings::new()?;
        let notion_api = notion::NotionApi::new(settings.notion.api_key.clone())?;

        Ok(Self {
            settings,
            notion_api,
            categories_cache: None,
            last_date: None,
        })
    }

    pub async fn run() -> Result<()> {
        let mut app = Self::new()?;

        let db: notion::models::Database = app
            .notion_api
            .get_database(&app.settings.notion.database_id)
            .await?;

        let confirm = inquire::Confirm::new("Want to add one more row?").with_default(true);

        let last5 = app
            .get_database_pages(
                &app.settings.notion.database_id,
                Some(database_sorting("Date", 5)),
            )
            .await?;

        for page in last5.iter().rev() {
            let date = page_property_to_string(&page, "Date").unwrap_or_default();
            let amount = page_property_to_string(&page, "Amount").unwrap_or_default();
            println!(
                "{} {} {}",
                date,
                page.title().unwrap_or("Untitled".to_string()),
                amount
            );
        }

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
        query: Option<notion::models::search::DatabaseQuery>,
    ) -> Result<Vec<notion::models::Page>> {
        let result = self
            .notion_api
            .query_database(database_id, query.unwrap_or_default())
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

            preselect = self.settings.get(name.as_ref());

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
            let now = notion::chrono::offset::Local::now().date_naive();
            let default_date = self.last_date.unwrap_or(now);

            let date = inquire::DateSelect::new("Date:")
                .with_default(default_date)
                .with_min_date(
                    now.checked_sub_days(notion::chrono::Days::new(7)).unwrap(),
                )
                .with_max_date(now)
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

            self.last_date = Some(date);
        }

        if let Some(notion::models::properties::PropertyConfiguration::Relation { id, relation }) =
            db_properties.get("Category")
        {
            if self.categories_cache.is_none() {
                self.categories_cache = self
                    .get_database_pages(&relation.database_id, None)
                    .await
                    .ok();
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
