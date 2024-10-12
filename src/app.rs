use eyre::Result;
use inquire::{autocompletion::Replacement, Autocomplete};
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

fn database_sorting(
    property: impl Into<String>,
    page_size: u8,
) -> notion::models::search::DatabaseQuery {
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

#[derive(Clone)]
struct TitleCompleter {
    input: String,
    prev_titles: Vec<String>,
    output: Vec<String>,
}

impl TitleCompleter {
    fn new(titles: Vec<&str>) -> Self {
        Self {
            prev_titles: titles.into_iter().map(|t| t.to_owned()).collect(),
            ..Default::default()
        }
    }

    fn update_input(&mut self, input: &str) -> Result<(), inquire::CustomUserError> {
        if self.input == input {
            return Ok(());
        }

        self.input = input.to_owned();
        self.output.clear();

        for item in &self.prev_titles {
            if item.starts_with(input) {
                self.output.push(item.to_string());
            }
        }

        Ok(())
    }
}

impl Default for TitleCompleter {
    fn default() -> Self {
        Self {
            input: "".to_string(),
            output: vec![],
            prev_titles: vec![],
        }
    }
}

impl Autocomplete for TitleCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        self.update_input(input)?;

        Ok(self.output.clone())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, inquire::CustomUserError> {
        self.update_input(input)?;

        Ok(match highlighted_suggestion {
            Some(suggestion) => Replacement::Some(suggestion),
            None => {
                if self.output.len() == 1 {
                    Replacement::Some(self.output[0].clone())
                } else {
                    Replacement::None
                }
            }
        })
    }
}

#[derive(Debug)]
enum Operator {
    Add,
    Sub,
    Mul,
    Div,
}

impl Operator {
    fn from(c: &char) -> Option<Self> {
        match c {
            '+' => Some(Self::Add),
            '-' => Some(Self::Sub),
            '*' => Some(Self::Mul),
            '/' => Some(Self::Div),
            _ => None,
        }
    }
}

fn calc(expresion: &str) -> Result<f64> {
    let mut op: Option<Operator> = None;
    let mut pos: Option<usize> = None;
    let mut stack: Vec<f64> = Vec::new();

    for (i, c) in expresion.char_indices() {
        if char::is_digit(c, 10) && pos.is_none() {
            pos.replace(i);
        } else if !char::is_digit(c, 10) && !matches!(c, ','|'.') {
            if pos.is_some() {
                let v = expresion[pos.take().unwrap()..i].parse()?;
                stack.push(v);

                if op.is_some() && stack.len() == 2 {
                    let rhs: f64 = stack.remove(1);
                    let lhs: f64 = stack.remove(0);

                    let result = match op.take().unwrap() {
                        Operator::Add => lhs + rhs,
                        Operator::Sub => lhs - rhs,
                        Operator::Mul => lhs * rhs,
                        Operator::Div => lhs / rhs,
                    };

                    stack.push(result);
                }
            }
            op = Operator::from(&c);
        }

    }

    if pos.is_some() {
        let v = expresion[pos.take().unwrap()..].parse()?;
        stack.push(v);
    }

    if op.is_some() && stack.len() == 2 {
        let rhs: f64 = stack.remove(1);
        let lhs: f64 = stack.remove(0);

        let result = match op.take().unwrap() {
            Operator::Add => lhs + rhs,
            Operator::Sub => lhs - rhs,
            Operator::Mul => lhs * rhs,
            Operator::Div => lhs / rhs,
        };

        return Ok(result);
    } else if op.is_none() && stack.len() == 1 {
        return Ok(stack.remove(0));
    }

    Err(eyre::Error::msg("Not expeceted"))
}

#[test]
fn calc_test() {
    let result = calc("10+10").unwrap();
    assert_eq!(result, 20.0);

    let result = calc("10+10+10").unwrap();
    assert_eq!(result, 30.0);

    let result = calc("10+10.1+10").unwrap();
    assert_eq!(result, 30.1);

    let result = calc("10+10+10-30").unwrap();
    assert_eq!(result, 0.0);

    let result = calc("10-30+10+10+3").unwrap();
    assert_eq!(result, 3.0);

    let result = calc("10/2").unwrap();
    assert_eq!(result, 5.0);

    let result = calc("10*2").unwrap();
    assert_eq!(result, 20.0);

    let result = calc("10").unwrap();
    assert_eq!(result, 10.0);

    let result = calc("10.1").unwrap();
    assert_eq!(result, 10.1);

    // let result = calc("10+(10*3)").unwrap();
    // assert_eq!(result, 40.0);

    let result = calc("10-10").unwrap();
    assert_eq!(result, 0.0);

    let result = calc("10*10").unwrap();
    assert_eq!(result, 100.0);

    let result = calc("10/10").unwrap();
    assert_eq!(result, 1.0);
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
            let name = inquire::Text::new("Name:")
                .with_autocomplete(TitleCompleter::new(self.settings.list()))
                .prompt()?;

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
            let amount = calc(&amount)?;

            properties.insert(
                "Amount".to_string(),
                notion::models::properties::PropertyValue::Number {
                    id: id.clone(),
                    number: serde_json::Number::from_f64(amount),
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
                .with_min_date(now.checked_sub_days(notion::chrono::Days::new(30)).unwrap())
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
