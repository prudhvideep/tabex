use std::{error::Error, fs::File, io::Write, time::Instant};

use clap::{App, Arg};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct TableMetadata {
    id: Option<String>,
    class: Option<String>,
    caption: Option<String>,
    position: usize,
    row_count: usize,
    column_count: usize,
    header_row_count: usize,
    footer_row_count: usize,
    parent_section: Option<String>,
    preceding_heading: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TableData {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Table {
    metadata: TableMetadata,
    data: TableData,
}

#[derive(Debug, Serialize, Deserialize)]
struct PageMetadata {
    url: String,
    title: Option<String>,
    description: Option<String>,
    author: Option<String>,
    published_date: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractionResult {
    page: PageMetadata,
    tables: Vec<Table>,
    extraction_time_ms: u128,
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = App::new("Web Table Extractor")
        .version("1.0")
        .author("Your Name")
        .about("Extracts tables and metadata from websites")
        .arg(
            Arg::with_name("url")
                .short("u")
                .long("url")
                .value_name("URL")
                .help("URL of the website to extract tables from")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("Output file (default is stdout)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("format")
                .short("f")
                .long("format")
                .value_name("FORMAT")
                .help("Output format (json or csv)")
                .default_value("json")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("user-agent")
                .long("user-agent")
                .value_name("AGENT")
                .help("User agent string to use for requests")
                .default_value("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .takes_value(true),
        )
        .get_matches();

    let url = matches.value_of("url").unwrap();
    let format = matches.value_of("format").unwrap();
    let user_agent = matches.value_of("user-agent").unwrap();

    // Start timing
    let start = Instant::now();

    // Fetch and parse the web page
    println!("Fetching URL: {}", url);
    let client = reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .build()?;

    let resp = client.get(url).send()?;

    if !resp.status().is_success() {
        return Err(format!("Failed to fetch URL: HTTP {}", resp.status()).into());
    }

    let html_content = resp.text()?;
    let document = Html::parse_document(&html_content);

    // Extract page metadata
    let page_metadata = extract_page_metadata(&document, url);

    // Extract tables
    let tables = extract_tables(&document);

    // Calculate extraction time
    let extraction_time = start.elapsed().as_millis();

    // Prepare result
    let result = ExtractionResult {
        page: page_metadata,
        tables,
        extraction_time_ms: extraction_time,
    };

    // Output results
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&result)?;
            if let Some(output_file) = matches.value_of("output") {
                let mut file = File::create(output_file)?;
                file.write_all(json.as_bytes())?;
                println!("Results written to {}", output_file);
            } else {
                println!("{}", json);
            }
        }
        "csv" => {
            if let Some(output_file) = matches.value_of("output") {
                output_tables_as_csv(&result, output_file)?;
                println!("Results written to {}", output_file);
            } else {
                output_tables_as_csv_to_stdout(&result)?;
            }
        }
        _ => return Err("Unsupported output format".into()),
    }

    // Print summary
    println!("\nExtraction Summary:");
    println!("URL: {}", url);
    println!("Tables found: {}", result.tables.len());
    println!("Extraction time: {} ms", extraction_time);

    Ok(())
}

fn extract_page_metadata(document: &Html, url: &str) -> PageMetadata {
    // Helper function to get meta tag content
    let get_meta_content = |name: &str| {
        let selector =
            Selector::parse(&format!("meta[name='{}'], meta[property='{}']", name, name)).unwrap();
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(String::from)
    };

    // Extract title
    let title_selector = Selector::parse("title").unwrap();
    let title = document
        .select(&title_selector)
        .next()
        .map(|el| el.inner_html().trim().to_string());

    // Extract other metadata
    let description =
        get_meta_content("description").or_else(|| get_meta_content("og:description"));
    let author = get_meta_content("author");
    let published_date =
        get_meta_content("article:published_time").or_else(|| get_meta_content("pubdate"));
    let last_modified =
        get_meta_content("article:modified_time").or_else(|| get_meta_content("lastmod"));

    PageMetadata {
        url: url.to_string(),
        title,
        description,
        author,
        published_date,
        last_modified,
    }
}

fn extract_tables(document: &Html) -> Vec<Table> {
    let table_selector = Selector::parse("table").unwrap();
    let h_selector = Selector::parse("h1, h2, h3, h4, h5, h6").unwrap();
    let caption_selector = Selector::parse("caption").unwrap();
    let tr_selector = Selector::parse("tr").unwrap();
    let th_selector = Selector::parse("th").unwrap();
    let td_selector = Selector::parse("td").unwrap();
    let section_selector = Selector::parse("section, article, div[role='main']").unwrap();

    let mut tables = Vec::new();
    let mut table_position = 0;

    for table_element in document.select(&table_selector) {
        table_position += 1;

        // Get table attributes
        let id = table_element.value().attr("id").map(String::from);
        let class = table_element.value().attr("class").map(String::from);

        // Get caption
        let caption = table_element
            .select(&caption_selector)
            .next()
            .map(|cap| cap.inner_html().trim().to_string());

        // Get parent section
        let parent_section = find_parent_with_selector(table_element.clone(), &section_selector)
            .and_then(|section| {
                section
                    .value()
                    .attr("id")
                    .or_else(|| section.value().attr("class"))
            })
            .map(String::from);

        // Find preceding heading
        let preceding_heading =
            find_preceding_heading(table_element.clone(), document, &h_selector);

        // Process rows
        let rows_elements: Vec<_> = table_element.select(&tr_selector).collect();
        let row_count = rows_elements.len();

        // Count header and footer rows
        let header_row_count = rows_elements
            .iter()
            .take_while(|row| row.select(&th_selector).next().is_some())
            .count();

        // Count footer rows (rows in tfoot or with th elements at end)
        let footer_row_count = rows_elements
            .iter()
            .rev()
            .take_while(|row| {
                let is_in_tfoot = find_parent_with_tag((*row).clone(), "tfoot").is_some();
                is_in_tfoot || row.select(&th_selector).next().is_some()
            })
            .count();

        let data_row_count = if row_count > header_row_count + footer_row_count {
            row_count - header_row_count - footer_row_count
        } else {
            0 // Fallback to 0 if counts are invalid
        };

        // Extract headers
        let headers = if header_row_count > 0 {
            rows_elements[0]
                .select(&th_selector)
                .map(|cell| clean_cell_text(cell.inner_html()))
                .collect::<Vec<String>>() // Using turbofish
        } else {
            Vec::new()
        };

        // Count columns based on the row with the most cells
        let column_count = rows_elements
            .iter()
            .map(|row| row.select(&th_selector).count() + row.select(&td_selector).count())
            .max()
            .unwrap_or(0);

        // Extract data rows
        let data_rows: Vec<Vec<String>> = rows_elements
            .iter()
            .skip(header_row_count)
            .take(data_row_count)
            .map(|row| {
                row.select(&td_selector)
                    .map(|cell| clean_cell_text(cell.inner_html()))
                    .collect()
            })
            .collect();

        // Create table object
        let table = Table {
            metadata: TableMetadata {
                id,
                class,
                caption,
                position: table_position,
                row_count,
                column_count,
                header_row_count,
                footer_row_count,
                parent_section,
                preceding_heading,
            },
            data: TableData {
                headers,
                rows: data_rows,
            },
        };

        tables.push(table);
    }

    tables
}

fn find_parent_with_selector<'a>(
    element: scraper::ElementRef<'a>,
    selector: &Selector,
) -> Option<scraper::ElementRef<'a>> {
    let mut current = element;

    while let Some(parent_node) = current.parent() {
        if let Some(parent_element) = scraper::ElementRef::wrap(parent_node) {
            if selector.matches(&parent_element) {
                return Some(parent_element);
            }
            current = parent_element;
        } else {
            // If parent is not an element, skip it
            current = scraper::ElementRef::wrap(parent_node.parent()?)?;
        }
    }
    None
}

fn find_parent_with_tag<'a>(
    element: scraper::ElementRef<'a>,
    tag_name: &str,
) -> Option<scraper::ElementRef<'a>> {
    let mut current = element;

    while let Some(parent_node) = current.parent() {
        if let Some(parent_element) = scraper::ElementRef::wrap(parent_node) {
            if parent_element.value().name().eq_ignore_ascii_case(tag_name) {
                return Some(parent_element);
            }
            current = parent_element;
        } else {
            // If parent is not an element, skip it
            current = scraper::ElementRef::wrap(parent_node.parent()?)?;
        }
    }
    None
}

fn find_preceding_heading(
    element: scraper::ElementRef,
    document: &Html,
    h_selector: &Selector,
) -> Option<String> {
    // This is a simplified approach - ideally you'd traverse the DOM tree
    // For simplicity, we'll just get all headings and find the last one before our table
    let all_headings: Vec<_> = document.select(h_selector).collect();
    let all_elements: Vec<_> = document.select(&Selector::parse("*").unwrap()).collect();

    let table_pos = all_elements.iter().position(|&el| el == element)?;

    all_headings
        .into_iter()
        .filter_map(|heading| {
            let heading_pos = all_elements.iter().position(|&el| el == heading)?;
            if heading_pos < table_pos {
                Some((heading_pos, heading.inner_html().trim().to_string()))
            } else {
                None
            }
        })
        .max_by_key(|(pos, _)| *pos)
        .map(|(_, text)| text)
}

fn clean_cell_text(html: String) -> String {
    // Remove HTML tags
    let re = Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(&html, "");

    // Normalize whitespace
    let ws_re = Regex::new(r"\s+").unwrap();
    let text = ws_re.replace_all(&text, " ");

    text.trim().to_string()
}

fn output_tables_as_csv(
    result: &ExtractionResult,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    let mut file = File::create(output_file)?;

    // Write metadata as a comment
    writeln!(file, "# URL: {}", result.page.url)?;
    if let Some(title) = &result.page.title {
        writeln!(file, "# Title: {}", title)?;
    }
    writeln!(file, "# Tables found: {}", result.tables.len())?;
    writeln!(file, "# Extraction time: {} ms", result.extraction_time_ms)?;
    writeln!(file)?;

    // Write each table
    for (i, table) in result.tables.iter().enumerate() {
        writeln!(file, "# Table {} of {}", i + 1, result.tables.len())?;
        writeln!(file, "# Position: {}", table.metadata.position)?;
        if let Some(caption) = &table.metadata.caption {
            writeln!(file, "# Caption: {}", caption)?;
        }
        if let Some(heading) = &table.metadata.preceding_heading {
            writeln!(file, "# Preceding heading: {}", heading)?;
        }
        writeln!(file)?;

        // Write headers
        if !table.data.headers.is_empty() {
            writeln!(file, "{}", table.data.headers.join(","))?;
        }

        // Write data rows
        for row in &table.data.rows {
            writeln!(file, "{}", row.join(","))?;
        }

        // Add separator between tables
        if i < result.tables.len() - 1 {
            writeln!(file)?;
            writeln!(file, "# ------------------------------")?;
            writeln!(file)?;
        }
    }

    Ok(())
}

fn output_tables_as_csv_to_stdout(result: &ExtractionResult) -> Result<(), Box<dyn Error>> {
    // Write metadata as a comment
    println!("# URL: {}", result.page.url);
    if let Some(title) = &result.page.title {
        println!("# Title: {}", title);
    }
    println!("# Tables found: {}", result.tables.len());
    println!("# Extraction time: {} ms", result.extraction_time_ms);
    println!();

    // Write each table
    for (i, table) in result.tables.iter().enumerate() {
        println!("# Table {} of {}", i + 1, result.tables.len());
        println!("# Position: {}", table.metadata.position);
        if let Some(caption) = &table.metadata.caption {
            println!("# Caption: {}", caption);
        }
        if let Some(heading) = &table.metadata.preceding_heading {
            println!("# Preceding heading: {}", heading);
        }
        println!();

        // Write headers
        if !table.data.headers.is_empty() {
            println!("{}", table.data.headers.join(","));
        }

        // Write data rows
        for row in &table.data.rows {
            println!("{}", row.join(","));
        }

        // Add separator between tables
        if i < result.tables.len() - 1 {
            println!();
            println!("# ------------------------------");
            println!();
        }
    }

    Ok(())
}
