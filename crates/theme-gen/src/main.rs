use clap::Parser;

type MainResult<T = ()> = anyhow::Result<T>;

fn main() -> MainResult {
    let cli = theme_gen::Cli::parse();
    let generated = theme_gen::run_cli(cli)?;
    println!("Generated {generated} theme files");
    Ok(())
}
