use std::{error::Error, fs::File, io::{BufReader, BufWriter, ErrorKind}, process};
use clap::{Parser, Subcommand, ValueEnum};
use exif_geolocation_util::{*};

#[derive(Debug,Parser)]
#[command(name="exif-geolocation-util")]
struct Cli {
    /// The path of the database file to read
    in_file: String,
    /// The path of the database file to write to
    #[arg(long="out")]
    out_file: Option<String>,
    /// Allow the source file to be overwritten
    #[arg(long)]
    overwrite: bool,
    #[command(subcommand)]
    command: Commands
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print general information about the database
    Info,
    /// Print lists of a certain type of entry, may produce large outputs
    List {
        /// The type of database entry to list
        #[arg(value_enum)]
        entry_type: EntryTypePlural,
    },
    /// Print details about specific entries
    Find {
        /// The type of database entry to search for
        #[arg(value_enum)]
        entry_type: EntryType,
        /// the name to search for
        /// 
        /// Can be specified as a comma separated list where the first item is always the name
        /// of the entry and subsequent items are parsed last to first in order or significance.
        /// 
        /// For example when searching for a city all of the following are possible:
        ///  * find city "<city>" e.g. "Timbuktu"
        ///  * find city "<city>, <country>" e.g. "Bristol, GB" or "Bristol, United Kingdom"
        ///  * find city "<city>, <region>, <country>" e.g. "Wick, England, GB"
        ///  * find city "<city>, <sub-region>, <region>, <country>" e.g. "Kingswood, South Gloucestershire, England, GB"
        #[arg(verbatim_doc_comment)]
        name: String,
        /// The maximum number of entries to print
        #[arg(long,default_value="4")]
        max_displayed: usize
    },
    /// Add a new entry to the database
    Add {
        /// the type of database entry to add, currently only "city" is supported
        #[arg(value_enum)]
        entry_type: EntryType,
        /// the name of the new entry
        name: String,
        /// the position of the new entry
        /// 
        /// Can be given in degree, minute, second (with decimal seconds) format; degree, minute (with decimal
        /// minutes) format; or decimal degree format. Degrees are are represented with the degree symbol, "°",
        /// or "deg" or "d". Minutes are represented with a single quotation mark / apostrophe (U+0027, U+2018
        /// or U+2019), "min" or "m". Seconds are represented with a double quotation mark (U+0022, U+201C
        /// or U+201D), "sec" or "s". When in decimal degrees format, the degrees symbol is optional.
        /// 
        /// Point Nemo, the oceanic pole of inaccessibility can be specified in any of the following ways:
        ///  * 48°52'36.0"S, 123°23'36.0"W
        ///  * 48deg 52min 36.0sec S, 123deg 23min 36.0sec W
        ///  * 48d 52m 36.0s S, 123d 23m 36.0s W
        ///  * -48d 52m 36.0s N, -123d 23m 36.0s E
        ///  * 48° 52.6' S, 123° 23.6' W
        ///  * 48deg 52.6min S, 123deg 23.6min W
        ///  * 48d 52.6m S, 123d 23.6m W
        ///  * -48d 52.6m N, -123d 23.6m E
        ///  * 48.88° S, 123.39° W
        ///  * 48.88deg S, 123.39deg W
        ///  * 48.88d S, 123.39d W
        ///  * 48.88 S, 123.39 W
        ///  * -48.88 N, -123.39 E
        ///  * -48.88, -123.39
        /// 
        /// Note that although decimal seconds is permitted, each latitude and longitude is packed into 
        /// 20 bits which makes the precision (smallest increment) of a latitude and logitude value 
        /// approximately 0.6 seconds and 1.2 seconds respectively. This corresponds to a smallest 
        /// increment of 38 metres (East or West) at the equator, 35 metres at latitude 23° (either 
        /// North or South), 27 metres at latitude 45° and 15 metres at latitude 67°.
        #[arg(short,long,verbatim_doc_comment)]
        position: String,
        /// the sub-region containing the city
        /// 
        /// Must be unique. Can further specify with the country, "<sub-region>, <country>", or the country and region,
        /// "<sub-region>, <region>, <country>".
        #[arg(short,long)]
        sub_region: String,
        /// the region containing the city
        /// 
        /// If not specified the region will be determined based on the specified sub-region
        #[arg(short,long)]
        region: Option<String>,
        /// the country containing the city
        /// 
        /// If not specified the country will be determined based on the specified sub-region
        #[arg(short,long)]
        country: Option<String>,
        /// the timezone containing the city
        /// 
        /// If not specified the timezone will be determined by finding the timezone of the first existant city
        /// in this database in the same sub-region
        #[arg(short,long)]
        timezone: Option<String>,
        /// the type of this feature
        /// 
        /// For a list of features try "exif_geolocation_util <database-file> list features"
        #[arg(short, long, default_value="Other")]
        feature_type: String,
        /// the population of the city
        /// 
        /// Expected in standard form
        #[arg(long, default_value="0.0e+0")]
        population: String
    },
    /// Remove a single entries
    Remove {
        /// The type of database entry to remove
        #[arg(value_enum)]
        entry_type: EntryType,
        /// the name of the entry to remove
        /// 
        /// Can be specified as a comma separated list where necessary to differentiate between entries. See
        /// the help text for the find command for details.
        name: String,
    },
}
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
enum EntryType {
    City, SubRegion, Region, Country
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
enum EntryTypePlural {
    Cities, SubRegions, Regions, Countries, Timezones, Features
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    // open file
    let f = File::open(&args.in_file).unwrap_or_else(|err| {
        eprint!("Error: ");
        match err.kind() {
            ErrorKind::NotFound => eprintln!("Source file not found"),
            _ => eprintln!("Source file could not be opened")
        }
        process::exit(1);
    });
    let mut reader = BufReader::new(f);

    // read database
    let mut database = GeoDatabase::read_from(&mut reader).unwrap_or_else(|err| {
        eprint!("Error: ");
        match err {
            DatabaseReadError::UnsupportedVersion { expected, found } => {
                eprintln!("Database version is not supported, expected {expected} found {found}");
            },
            DatabaseReadError::InvalidHeader { msg } => {
                eprintln!("Invalid database header: {msg}");
            },
            DatabaseReadError::IoError { source } => {
                eprintln!("There was an IO error whilst reading the database: {}", source)
            }
        }
        process::exit(1);
    });

    // run action
    let mut write_out = false;
    match args.command {
        Commands::Info {  } => {
            database.print_info();
        },
        Commands::List { entry_type } => {
            match entry_type {
                EntryTypePlural::Cities => database.print_cities(),
                EntryTypePlural::SubRegions => database.print_subregions(),
                EntryTypePlural::Regions => database.print_regions(),
                EntryTypePlural::Countries => database.print_countires(),
                EntryTypePlural::Timezones => database.print_timezones(),
                EntryTypePlural::Features => database.print_features(),
            }
        },
        Commands::Find { entry_type, name, max_displayed } => {
            match entry_type {
                EntryType::City => database.print_matching_cities(&name, max_displayed),
                EntryType::SubRegion => database.print_matching_subregion(&name, max_displayed),
                EntryType::Region => database.print_matching_regions(&name, max_displayed),
                EntryType::Country => database.print_matching_country(&name, max_displayed)
            }
        },
        Commands::Add {
            entry_type, 
            name, position, 
            sub_region, region, country, 
            timezone, feature_type,
            population
        } => {
            write_out = true;
            match entry_type {
                EntryType::City => {

                    // position
                    let (lat, long) = match parse_pos_string(&position) {
                        Ok((lat, long)) => (lat, long),
                        Err(err) => {
                            eprintln!("Invalid position: {err}");
                            process::exit(1);
                        }
                    };

                    // subregion
                    let matching_subregions = database.find_matching_subregions(&sub_region);
                    let subregion_ix = match matching_subregions.len() {
                        1 => matching_subregions[0],
                        0 => {
                            eprintln!("No subregions match \"{}\"", sub_region);
                            process::exit(1);
                        }
                        n => {
                            eprintln!("Multiple ({n}) subregions matched \"{}\", you may need to further specify the sub-region with a country or with a \
                                region and country as \"<sub-region>, <country>\" or \"<sub-region>, <region>, <country>\"", sub_region);

                            if n <= 5 {
                                for subregion_ix in matching_subregions {
                                    eprintln!("{}", database.subregion_name(subregion_ix));
                                }
                            }
                            process::exit(1);
                        }
                    };

                    // region, country and timezone
                    let (mut region_ix, mut country_ix, mut timezone_ix) = database.subregion_parents(subregion_ix);

                    // region
                    if let Some(region_name) = region {
                        let matching_regions = database.find_matching_regions(&region_name);
                        region_ix = match matching_regions.len() {
                            1 => matching_regions[0],
                            0 => {
                                eprintln!("No regions match \"{}\"", region_name);
                                process::exit(1);
                            }
                            n => {
                                eprintln!("Multiple ({n}) regions matched \"{}\", you may need to further specify the region with a country \
                                    as \"<region>, <country>\"", region_name);
    
                                if n <= 5 {
                                    for region_ix in matching_regions {
                                        eprintln!("{}", database.region_name(region_ix));
                                    }
                                }
                                process::exit(1);
                            }
                        };
                    }

                    // country
                    if let Some(country_name) = country {
                        let matching_countries = database.find_matching_countries(&country_name);
                        country_ix = match matching_countries.len() {
                            1 => matching_countries[0],
                            0 => {
                                eprintln!("No regions match \"{}\"", country_name);
                                process::exit(1);
                            }
                            n => {
                                eprintln!("Multiple ({n}) countries matched \"{}\" try prefixing the country name with the two letter \
                                    country code e.g. \"GBUnited Kingdom\"", country_name);
    
                                if n <= 5 {
                                    for country_ix in matching_countries {
                                        eprintln!("{} ({})", database.country_name(country_ix), database.country_code(country_ix));
                                    }
                                }
                                process::exit(1);
                            }
                        };
                    }

                    // timezone
                    if let Some(timezone_name) = timezone {
                        let matching_timezones = database.find_matching_timezones(&timezone_name);
                        timezone_ix = match matching_timezones.len() {
                            1 => matching_timezones[0],
                            0 => {
                                eprintln!("No timezones match \"{}\"", timezone_name);
                                process::exit(1);
                            }
                            n => {
                                eprintln!("Multiple ({n}) timezones matched \"{}\" try writing the full name of the timezone e.g. \"Europe/London\"", timezone_name);
    
                                if n <= 5 {
                                    for timezone_ix in matching_timezones {
                                        eprintln!("{}", database.timezone_name(timezone_ix));
                                    }
                                }
                                process::exit(1);
                            }
                        };
                    }

                    // feature
                    let matching_features = database.find_matching_features(&feature_type);
                    let feature_ix = match matching_features.len() {
                        1 => matching_features[0],
                        0 => {
                            eprintln!("No features match \"{}\"", feature_type);
                            process::exit(1);
                        }
                        n => {
                            eprintln!("Multiple ({n}) features matched \"{}\" try writing the full name of the feature", feature_type);

                            if n <= 5 {
                                for feature_ix in matching_features {
                                    eprintln!("{}", database.feature_name(feature_ix));
                                }
                            }
                            process::exit(1);
                        }
                    };

                    // population
                    let population = parse_population_string(&population).unwrap_or_else(|err| {
                        eprintln!("Invalid population: {err}");
                        process::exit(1);
                    });

                    println!("----------------- New Entry ------------------");
                    println!("        name: {name}");
                    println!("    position: {lat:.2}°, {long:.2}°");
                    println!("   subregion: {} ({subregion_ix})", database.subregion_name(subregion_ix));
                    println!("      region: {} ({region_ix})", database.region_name(region_ix));
                    println!("     country: {} ({country_ix})", database.country_name(country_ix));
                    println!("    timezone: {} ({timezone_ix})", database.timezone_name(timezone_ix));
                    println!("     feature: {} ({feature_ix})", database.feature_name(feature_ix));
                    println!("  population: {} (0x{population:X})", format_population(population));
                    println!("----------------------------------------------");

                    let city = CityEntry{
                        name, latitude: lat, longitude: long, population, country_ix, region_ix, subregion_ix, timezone_ix, feature_ix
                    };

                    database.add_city(city);
                }
                _ => {
                    eprintln!("Adding non-city entries is not supported currently");
                    process::exit(1);
                }
            }
        },
        Commands::Remove { entry_type, name } => {
            write_out = true;
            match entry_type {
                EntryType::City => {
                    let matching_cities = database.find_matching_cities(&name);
                    match matching_cities.len() {
                        1 => {
                            database.remove_city(matching_cities[0]);
                        },
                        0 => {
                            eprintln!("No cities were found matching \"{name}\"");
                            process::exit(1);
                        },
                        n => {
                            eprintln!("Multiple ({n}) cities matched \"{name}\", you may need to provide greater specificity");
                            process::exit(1);
                        }
                    }
                },
                _ => {
                    eprintln!("Only removing cities is supported currently");
                    process::exit(1);
                }
            }
        },
    }

    if write_out {
        let out_file = if args.overwrite {
            args.in_file
        } else if let Some(out_file) = args.out_file {
            out_file
        } else {
            eprintln!("No output file path given and overwrite flag not set: use the \"--out <path>\" option to specify an output file or provide the \"--overwrite\" flag to permit writing to the source file.");
            process::exit(1);
        };

        // open output file
        let f = File::create(&out_file).unwrap_or_else(|err| {
            eprint!("Error: output file could not be opened: {}", err);
            process::exit(1);
        });
        let mut writer = BufWriter::new(f);

        // write database
        database.write_to(&mut writer).unwrap_or_else(|err| {
            eprintln!("Error writing database: {}", err);
            process::exit(1);
        });
    }

    /*let file_path = "D:\\Documents\\Projects\\geolocation\\Geolocation.dat";
    let f = File::open(file_path)?;
    let mut reader = BufReader::new(f);

    let database = read_database(&mut reader)?;

    let file_path_out = "D:\\Documents\\Projects\\geolocation\\Geolocation_out.dat";
    let f = File::create(file_path_out)?;
    let mut writer = BufWriter::new(f);

    write_database(&mut writer, &database)?;*/

    Ok(())
}
