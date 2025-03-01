use clap::{Parser, Subcommand};
use fcb_core::error::Error;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer,
    header_writer::HeaderWriterOptions,
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
    Ser {
        /// Input file (use '-' for stdin)
        #[arg(short, long)]
        input: String,

        /// Output file (use '-' for stdout)
        #[arg(short, long)]
        output: String,

        /// Comma-separated list of attributes to create index for
        #[arg(long)]
        attr_index: Option<String>,
    },

    /// Convert FCB to CityJSON
    Deser {
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

fn get_reader(input: &str) -> Result<Box<dyn Read>, Error> {
    match input {
        "-" => Ok(Box::new(io::stdin())),
        path => Ok(Box::new(File::open(path)?)),
    }
}

fn get_writer(output: &str) -> Result<Box<dyn Write>, Error> {
    match output {
        "-" => Ok(Box::new(io::stdout())),
        path => Ok(Box::new(File::create(path)?)),
    }
}

fn serialize(input: &str, output: &str, attr_index: Option<String>) -> Result<(), Error> {
    let reader = BufReader::new(get_reader(input)?);
    let writer = BufWriter::new(get_writer(output)?);

    let cj_seq = match read_cityjson_from_reader(reader, CJTypeKind::Seq) {
        Ok(CJType::Seq(seq)) => seq,
        _ => {
            return Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "failed to read CityJSON Feature",
            )))
        }
    };

    let CityJSONSeq { cj, features } = cj_seq;
    let attr_schema = {
        let mut schema = AttributeSchema::new();
        for feature in features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    schema.add_attributes(attributes);
                }
            }
        }
        if schema.is_empty() {
            None
        } else {
            Some(schema)
        }
    };

    let attr_index_vec = attr_index.map(|s| {
        s.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    let header_options = HeaderWriterOptions {
        write_index: true,
        feature_count: features.len() as u64,
        index_node_size: 16,
        attribute_indices: attr_index_vec,
    };

    let mut fcb = FcbWriter::new(cj, Some(header_options), attr_schema)?;

    for feature in features.iter() {
        fcb.add_feature(feature)?;
    }
    fcb.write(writer)?;

    if output != "-" {
        eprintln!("Successfully encoded to FCB");
    }
    Ok(())
}

fn deserialize(input: &str, output: &str) -> Result<(), Error> {
    let reader = BufReader::new(get_reader(input)?);
    let mut writer = BufWriter::new(get_writer(output)?);
    let mut fcb_reader = FcbReader::open(reader)?.select_all_seq()?;

    let header = fcb_reader.header();
    let cj = deserializer::to_cj_metadata(&header)?;

    // Write header
    writeln!(writer, "{}", serde_json::to_string(&cj)?)?;

    // Write features
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = fcb_reader.next() {
        let feature = feat_buf.cur_cj_feature()?;
        writeln!(writer, "{}", serde_json::to_string(&feature)?)?;

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

fn show_info(input: PathBuf) -> Result<(), Error> {
    let reader = BufReader::new(File::open(input)?);
    let metadata = reader.get_ref().metadata()?.len() / 1024 / 1024; // show in megabytes
    let fcb_reader = FcbReader::open(reader)?.select_all()?;

    let header = fcb_reader.header();
    println!("FCB File Info:");
    println!("    File size: {} MB", metadata);
    println!("  Version: {}", header.version());
    println!("  Features count: {}", header.features_count());
    println!("  bbox: {:?}", header.geographical_extent());

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

fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ser {
            input,
            output,
            attr_index,
        } => serialize(&input, &output, attr_index),
        Commands::Deser { input, output } => deserialize(&input, &output),
        Commands::Info { input } => show_info(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
