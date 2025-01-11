use anyhow::Result;
use clap::{Parser, Subcommand};
use flatcitybuf::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    fcb_deserializer,
    header_writer::{HeaderMetadata, HeaderWriterOptions},
    read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq, FcbReader, FcbWriter,
};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(author, version, about = "CLI tool for CityJSON <-> FCB conversion")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert CityJSON to FCB
    Serialize {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,

        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,
    },

    /// Convert FCB to CityJSON
    Deserialize {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,

        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,
    },

    /// Show info about FCB file
    Info {
        /// Input FCB file
        #[arg(short, long)]
        input: PathBuf,
    },
}

fn get_reader(input: &str) -> Result<Box<dyn Read>> {
    match input {
        "-" => Ok(Box::new(io::stdin())),
        path => Ok(Box::new(File::open(path)?)),
    }
}

fn get_writer(output: &str) -> Result<Box<dyn Write>> {
    match output {
        "-" => Ok(Box::new(io::stdout())),
        path => Ok(Box::new(File::create(path)?)),
    }
}

fn serialize(input: &str, output: &str) -> Result<()> {
    let reader = BufReader::new(get_reader(input)?);
    let writer = BufWriter::new(get_writer(output)?);

    let cj_seq = match read_cityjson_from_reader(reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => anyhow::bail!("Expected CityJSONSeq"),
    };

    let CityJSONSeq { cj, features } = cj_seq;
    let mut attr_schema = AttributeSchema::new();
    for feature in features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }

    let header_metadata = HeaderMetadata {
        features_count: features.len() as u64,
    };
    let header_options = Some(HeaderWriterOptions {
        write_index: false,
        header_metadata,
    });
    let mut fcb = FcbWriter::new(
        cj,
        header_options,
        None,
        if attr_schema.is_empty() {
            None
        } else {
            Some(&attr_schema)
        },
    )?;
    fcb.write_feature()?;

    for feature in features.iter() {
        fcb.add_feature(feature)?;
    }
    fcb.write(writer)?;

    if output != "-" {
        eprintln!("Successfully encoded to FCB");
    }
    Ok(())
}

fn deserialize(input: &str, output: &str) -> Result<()> {
    let reader = BufReader::new(get_reader(input)?);
    let mut writer = BufWriter::new(get_writer(output)?);
    let mut fcb_reader = FcbReader::open(reader)?.select_all_seq()?;

    let header = fcb_reader.header();
    let cj = fcb_deserializer::to_cj_metadata(&header)?;

    // Write header
    writeln!(writer, "{}", serde_json::to_string(&cj)?)?;

    let root_attr_schema = header.columns();
    // Write features
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = fcb_reader.next() {
        let feature = feat_buf.cur_feature();
        let cj_feature = fcb_deserializer::to_cj_feature(feature, None)?;
        writeln!(writer, "{}", serde_json::to_string(&cj_feature)?)?;

        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }

    if output != "-" {
        eprintln!("Successfully decoded to CityJSON");
    }
    Ok(())
}

fn show_info(input: PathBuf) -> Result<()> {
    let reader = BufReader::new(File::open(input)?);
    let metadata = reader.get_ref().metadata()?.len() / 1024 / 1024; // show in megabytes
    let fcb_reader = FcbReader::open(reader)?.select_all()?;

    let header = fcb_reader.header();
    println!("FCB File Info:");
    println!("    File size: {} MB", metadata);
    println!("  Version: {}", header.version());
    println!("  Features count: {}", header.features_count());

    if let Some(title) = header.title() {
        println!("  Title: {}", title);
    }

    if let Some(extent) = header.geographical_extent() {
        println!("  Geographical extent:");
        println!(
            "    Min: [{}, {}, {}]",
            extent.min().x(),
            extent.min().y(),
            extent.min().z()
        );
        println!(
            "    Max: [{}, {}, {}]",
            extent.max().x(),
            extent.max().y(),
            extent.max().z()
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serialize { input, output } => serialize(&input, &output),
        Commands::Deserialize { input, output } => deserialize(&input, &output),
        Commands::Info { input } => show_info(input),
    }
}
