#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]

use std::{
    collections::HashMap,
    convert::Infallible,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

fn get_alpha_vantage_client() -> alpha_vantage::api::ApiClient {
    let api_key = &std::env::var("HLEDGER_GET_MARKET_PRICES_API_KEY").unwrap_or_else(|error| {
        match error {
            std::env::VarError::NotPresent => eprintln!("Environment variable HLEDGER_GET_MARKET_PRICES_API_KEY is not set.\nPlease set this variable to your Alpha Vantage API key and try again."),
            std::env::VarError::NotUnicode(_) => eprintln!("Environment variable HLEDGER_GET_MARKET_PRICES_API_KEY is not set.\nPlease recheck whether this variable is indeed set to your API key.")
        }

        std::process::exit(1);
    });

    let user_agent_for_http_requests = concat!(
        env!("CARGO_PKG_NAME"),
        " V",
        env!("CARGO_PKG_VERSION"),
        " (",
        env!("CARGO_PKG_REPOSITORY"),
        ")"
    );

    let reqwest_client = reqwest::Client::builder()
        .user_agent(user_agent_for_http_requests)
        .build()
        .unwrap_or_else(|error| {
            report_application_bug("Could not build reqwest client", Some(error))
        });

    alpha_vantage::set_api(api_key, reqwest_client)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "for every error type E, &E is also an error type, so passing `Option<&E>` is unnecessary complicated"
)]
fn report_application_bug<E: std::error::Error>(error_string: &str, error: Option<E>) -> ! {
    eprintln!("An unexpected problem occured that the application can't recover from.\n\nDetails about the error are below. If you believe the invocation of hledger-get-market-prices is correct, I'd appreciate a bug report at {}/issues/new.\n\nError message: {error_string}\nError: {error:?}", env!("CARGO_PKG_REPOSITORY"));

    std::process::exit(1);
}

pub async fn search_stock_symbol(search_query: String) {
    let search = get_alpha_vantage_client()
        .search(&search_query)
        .json()
        .await
        .unwrap_or_else(|error| {
            report_application_bug("alpha_vantage returned error during `search`", Some(error));
        });
    let matches = search.matches();
    println!("{:>20} | {:>9} – {:20}", "Region", "Symbol", "Name");
    println!();
    for result in matches {
        println!(
            "{:>20} | {:>9} – {:20}",
            result.region(),
            result.symbol(),
            result.name()
        );
    }
}

pub async fn get_history_for_stock(
    stock_symbol: String,
    stock_commodity_name: String,
    currency_commodity_name: String,
    journal_file: PathBuf,
    separator: char,
    decimal_digits: Option<usize>,
    currency_symbol_before: bool,
) {
    let stock_name = stock_commodity_name;
    let stock_times = get_alpha_vantage_client()
        .stock_time(
            alpha_vantage::stock_time::StockFunction::Daily,
            &stock_symbol,
        )
        .output_size(alpha_vantage::api::OutputSize::Compact)
        .json()
        .await
        .unwrap_or_else(|error| {
            report_application_bug(
                "alpha_vantage returned error during `stock_time`",
                Some(error),
            )
        });

    // The `api_data` hashmap uses the date (in format YYYY-MM-DD, as used by
    // the API as well as hledger) as key. As value, the string that should be
    // put behind the date in the journal file (commodity name and price) is
    // used. The idea behind this is that we need to merge this hashmap with the
    // current journal file contents, and we don't want to parse this file any
    // further than necessary to accomplish the merge.
    let api_data: HashMap<String, String> = stock_times
        .data()
        .iter()
        .map(|data_for_day| {
            (data_for_day.time().to_string(), {
                let price = data_for_day.close();
                let mut price_string: String = decimal_digits.map_or_else(
                    || format!("{price}"),
                    |decimal_digits| format!("{price:.decimal_digits$}"),
                );

                if separator != '.' {
                    price_string = price_string.replace('.', &separator.to_string());
                }

                if currency_symbol_before {
                    format!("{stock_name} {currency_commodity_name}{price_string}")
                } else {
                    format!("{stock_name} {price_string} {currency_commodity_name}")
                }
            })
        })
        .collect();

    if stock_times.data().len() != api_data.len() {
        report_application_bug::<Infallible>(
            &format!(
                "There are duplicate days in the API response: {} != {}",
                stock_times.data().len(),
                api_data.len()
            ),
            None,
        );
    }

    let file = File::open(&journal_file)
        .unwrap_or_else(|e| report_application_bug("Couldn't open journal file", Some(e)));
    let file_data: HashMap<_, _> = BufReader::new(file)
        .lines()
        .map(|line| {
            line.unwrap_or_else(|e| {
                report_application_bug("Getting line from journal file failed", Some(e))
            })
            .trim_start()
            .to_string()
        })
        .filter(|line| !line.starts_with(';')) // filter comment lines
        .map(|line| {
            let (first_part, last_part) = line.split_once(' ').unwrap_or_else(|| {
                report_application_bug::<Infallible>(&format!("Contains no space: {line}"), None);
            });
            if first_part != "P" {
                report_application_bug::<Infallible>(
                    &format!("{line} is not a market price"),
                    None,
                );
            }
            let (date, price_info) = last_part.split_once(' ').unwrap_or_else(|| {
                report_application_bug::<Infallible>(
                    &format!("Contains only one space: {line}"),
                    None,
                );
            });
            (date.to_string(), price_info.to_string())
        })
        .collect();

    let mut new_data = file_data;
    new_data.extend(api_data);

    let mut new_data: Vec<(String, String)> = new_data.into_iter().collect();
    new_data.sort_by(|(a, _), (b, _)| a.cmp(b).reverse());

    let mut file = File::create(&journal_file)
        .unwrap_or_else(|e| report_application_bug("Couldn't open journal file", Some(e)));

    writeln!(
        file,
        "; Generated by {}",
        concat!(env!("CARGO_PKG_NAME"), " V", env!("CARGO_PKG_VERSION"))
    )
    .unwrap_or_else(|e| report_application_bug("Failed writing to journal file", Some(e)));
    for (current_datetime, price_info) in &new_data {
        writeln!(file, "P {current_datetime} {price_info}")
            .unwrap_or_else(|e| report_application_bug("Failed writing to journal file", Some(e)));
    }
}
