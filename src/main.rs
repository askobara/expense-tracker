use eyre::Result;

mod app;
mod settings;

#[tokio::main]
async fn main() -> Result<()> {
    crate::app::App::run().await?;

    println!("Done");

    Ok(())
}
