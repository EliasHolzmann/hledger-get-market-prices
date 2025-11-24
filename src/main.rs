#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct App {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[clap(
        about = "Search for a stock symbol. \nUse this if you don't know the exact stock symbol of the stock you are interested in."
    )]
    SearchStockSymbol { search_query: String },
    #[clap(about = "Outputs historic market prices of a stock in a hledger compatible format.")]
    History {
        #[clap(help = "Symbol of the stock as given by the `history` subcommand")]
        stock_symbol: String,
        #[clap(help = "Commodity name to use for the stock")]
        stock_commodity_name: String,
        #[clap(help = "Commodity name to use for the currency the market prices is denoted in")]
        currency_commodity_name: String,
        #[clap(
            short,
            long,
            help = "Number of digits after the decimal point to return."
        )]
        decimal_digits: Option<usize>,
        #[clap(
            short,
            long,
            default_value = ".",
            help = "What character to use as decimal separator"
        )]
        separator: char,
        #[clap(
            short,
            long,
            help = "Whether to place the currency symbol before or after the amount."
        )]
        commodity_symbol_before: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match App::parse().command {
        Command::SearchStockSymbol { search_query } => {
            hledger_get_market_prices::search_stock_symbol(search_query).await;
        }
        Command::History {
            stock_symbol,
            stock_commodity_name,
            decimal_digits,
            separator,
            currency_commodity_name,
            commodity_symbol_before,
        } => {
            hledger_get_market_prices::get_history_for_stock(
                stock_symbol,
                stock_commodity_name,
                currency_commodity_name,
                separator,
                decimal_digits,
                commodity_symbol_before,
            )
            .await;
        }
    }

    Ok(())
}
