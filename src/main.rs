use std::collections::VecDeque;
use clap::Parser;

use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use itertools::{Itertools, MinMaxResult};
use ratatui::{prelude::*, widgets::*};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_hint = clap::ValueHint::CommandWithArguments)]
    run: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let graph_title = cli.run.as_ref().map(|r| {
        r.join(" ")
    }).unwrap_or("foo".to_string());


    // std::io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    terminal.clear()?;

    let cancel_token = CancellationToken::new();
    let cloned_cancel_token = cancel_token.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        cancel_token.cancel();
    });

    let input_stream = async_stream::stream! {
        match cli.run {
            Some(ref args) => {
                let joined_args: Vec<_> = args.iter().map(|arg| format!("\"{arg}\"")).collect();
                let mut command = tokio::process::Command::new("sh");
                command.arg("-c").arg(joined_args.join(" "));

                loop {
                    match command.output().await {
                        Ok(output) => {
                            if let Ok(output) = String::from_utf8(output.stdout) {
                                yield output;
                            }
                        }
                        Err(e) => {
                            eprintln!("Error {e}");
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            None => {
                let stdin = tokio::io::stdin();
                let reader = BufReader::new(stdin);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    yield line
                }
            }
        }
    }.map(|line| {
        line.trim().parse::<usize>().unwrap_or(0)
    });

    pin_mut!(input_stream);

    let mut data = VecDeque::with_capacity(32);

    loop {
        tokio::select! {
            _ = cloned_cancel_token.cancelled() => {
                break;
            }
            Some(v) = input_stream.next() => {
                // println!("Got input {v:?}");
                data.push_front(v);
                if data.len() > 32 {
                    data.pop_back();
                }
                terminal.draw(|frame| ui(frame, &data, &graph_title))?;
            },
            else => break,
        }
    }

    let new_size = terminal.size()?;
    terminal.set_cursor(new_size.width, new_size.height)?;

    Ok(())
}

fn ui(frame: &mut Frame, data: &VecDeque<usize>, title: &str) {
    let f32_data: Vec<(f64, f64)> = data.iter().enumerate().map(|(idx, value)| (idx as f64, *value as f64)).collect();
    let dataset = vec![
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&f32_data)
    ];

    let (bounds, labels) = y_axis_bounds(data);

    let chart = Chart::new(dataset)
        .block(Block::default().title(title).title_alignment(Alignment::Center))
        .x_axis(Axis::default()
            .style(Style::default().fg(Color::White))
            .bounds([0.0, 32.0])
            .labels(["Now", "Earlier"].iter().cloned().map(Span::from).collect()))
        .y_axis(Axis::default()
            .style(Style::default().fg(Color::White))
            .bounds(bounds)
            .labels(labels.iter().cloned().map(Span::from).collect()));

    frame.render_widget(chart, frame.size());
}

fn y_axis_bounds(data: &VecDeque<usize>) -> ([f64; 2], [String; 4]) {
    // Find the Y axis bounds for our chart.
    // This is trickier than the x-axis. We iterate through all our PlotData structs
    // and find the min/max of all the values. Then we add a 10% buffer to them.
    let (min, max) = match data
        .iter()
        .minmax()
    {
        MinMaxResult::OneElement(elm) => (*elm, *elm),
        MinMaxResult::MinMax(min, max) => (*min, *max),
        MinMaxResult::NoElements => (usize::MAX, 0_usize),
    };

    // Add a 10% buffer to the top and bottom
    let max_10_percent = (max * 10) / 100;
    let min_10_percent = (min * 10) / 100;
    let top = max + max_10_percent;
    let bottom = min - min_10_percent;
    let percent_increment = (top - bottom) / 25;

    let labels = [bottom, bottom + percent_increment, bottom + (percent_increment * 2), top].map(|v| v.to_string());

    ([bottom as f64, top as f64], labels)
}