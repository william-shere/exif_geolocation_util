use std::{collections::HashSet, io::{self, BufRead, Write}};
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use regex::Regex;

fn read_line(reader :&mut dyn BufRead) -> Result<String, io::Error> {
    let mut s = String::new();
    reader.read_line(&mut s)?;
    return Ok(s.trim_end().to_owned());
}

pub struct CityEntry {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub population: u16,
    pub country_ix: usize,
    pub region_ix: usize,
    pub subregion_ix: usize,
    pub timezone_ix: usize,
    pub feature_ix: usize
}

pub fn write_city_entry(writer: &mut dyn Write, city: &CityEntry) -> Result<(), io::Error> {
    let lat = (((city.latitude + 90.0) / 180.0) * f64::from(0x100000)) as u32;
    let long = (((city.longitude + 180.0) / 360.0) * f64::from(0x100000)) as u32;

    let lt = (lat >> 4) as u16;
    let f = ( ((lat & 0x0f) << 4) | (long & 0x0f) )as u8;
    let ln = (long >> 4) as u16;
    let code = ((city.country_ix as u32) << 24) | ((city.population as u32) << 12) | (city.region_ix as u32);
    let sn = city.subregion_ix as u16;
    let tn = (city.timezone_ix & 0xff) as u8;
    let ftn = (((city.timezone_ix & 0x100) as u8) << 7) | (city.feature_ix as u8);

    writer.write_u16::<NetworkEndian>(lt)?;
    writer.write_u8(f)?;
    writer.write_u16::<NetworkEndian>(ln)?;
    writer.write_u32::<NetworkEndian>(code)?;
    writer.write_u16::<NetworkEndian>(sn)?;
    writer.write_u8(tn)?;
    writer.write_u8(ftn)?;
    writeln!(writer, "{}", city.name)?;

    Ok(())
}

/// Format is, I believe, as follows:
/// * 20 bit latitute
/// * 20 bit longitude
/// * 8 bit country index
/// * 12 bit population (4 bit integer, 4 bit decimal, 4 bit significand)
/// * 12 bit region index
/// * 16 bit subregion index
/// * 9 bit timezone index
/// * 1 bit unused?
/// * 6 bit feature code index 
/// * newline terminated string name
pub fn parse_city_entry(data: &[u8;13], reader: &mut dyn BufRead) -> Result<CityEntry, io::Error> {
    let mut data_reader = io::Cursor::new(data);

    let lt = u32::from(data_reader.read_u16::<NetworkEndian>().unwrap());
    let f = u32::from(data_reader.read_u8().unwrap());
    let ln = u32::from(data_reader.read_u16::<NetworkEndian>().unwrap());
    let code = data_reader.read_u32::<NetworkEndian>().unwrap();
    let sn = data_reader.read_u16::<NetworkEndian>().unwrap();
    let tn = u16::from(data_reader.read_u8().unwrap());
    let ftn = u32::from(data_reader.read_u8().unwrap());

    let name = read_line(reader)?;

    let lat = (lt << 4) | (f >> 4);
    let long = (ln << 4) | (f & 0x0f);

    let lat_deg = (180.0 * (f64::from(lat) / f64::from(0x100000))) - 90.0;
    let long_deg = (360.0 * (f64::from(long) / f64::from(0x100000))) - 180.0;

    let country_ix = (code >> 24) as usize;

    let pop = (code >> 12 & 0xfff) as u16;

    let region_ix = (code & 0xfff) as usize;

    let subregion_ix = sn as usize;
    let timezone_ix = if (ftn & 0x80) != 0 {
        tn + 256
    } else {
        tn
    } as usize;

    let feature_ix = (ftn & 0x3f) as usize;

    Ok(CityEntry{ name, latitude: lat_deg, longitude: long_deg, population: pop, country_ix, region_ix, subregion_ix, timezone_ix, feature_ix })
}

pub struct GeoDatabase {
    comment:String,
    cities:Vec<CityEntry>,
    countries:Vec<String>,
    regions:Vec<String>,
    subregions:Vec<String>,
    timezones:Vec<String>,
    features:Vec<String>
}

fn dd_to_dms(dd:f64, if_pos:char, if_neg:char) -> String {
    let d = dd.trunc().abs() as i64;
    let m = dd.fract().abs() * 60.0;
    let s = m.fract() * 60.0;
    return format!("{}°{}'{:.2}\"{}", d, m.trunc() as i8, s, if dd >= 0.0 { if_pos } else { if_neg });
}

fn dd_string_to_dd(sign: &str, deg: &str, dir: &str, max_abs: f64) -> Result<f64, &'static str> {
    let dd = deg.parse::<f64>().or(Err("degrees not a valid decimal number"))?;
    return parse_dd(sign, dd, dir, max_abs);
}

fn dm_string_to_dd(sign: &str, deg: &str, min: &str, dir: &str, max_abs: f64) -> Result<f64, &'static str> {
    let deg = deg.parse::<i32>().or(Err("degrees not a valid integer"))?;
    let min = min.parse::<f64>().or(Err("minutes not a valid decimal number"))?;
    if min < 0.0 || min >= 60.0 {
        return Err("minutes must be between 0 inclusive and 60 exclusive");
    }
    return parse_dd(sign, f64::from(deg) + (min / 60.0), dir, max_abs);
}

fn dms_string_to_dd(sign: &str, deg: &str, min: &str, sec: &str, dir: &str, max_abs: f64) -> Result<f64, &'static str> {
    let deg = deg.parse::<i32>().or(Err("degrees not a valid integer"))?;
    let min = min.parse::<i32>().or(Err("minutes not a valid integer"))?;
    if min < 0 || min >= 60 {
        return Err("minutes must be between 0 inclusive and 60 exclusive");
    }
    let sec = sec.parse::<f64>().or(Err("Seconds not a valid decimal number"))?;
    if sec < 0.0 || sec >= 60.0 {
        return Err("seconds must be between 0 inclusive and 60 exclusive");
    }
    return parse_dd(sign, f64::from(deg) + ((f64::from(min) + (sec / 60.0)) / 60.0), dir, max_abs);
}

fn parse_dd(sign: &str, dd: f64, dir: &str, max_abs: f64) -> Result<f64, &'static str> {
    let mut dd = dd;
    if sign == "-" {
        if dir == "S" || dir == "W" {
            return Err("a negative angle south or west is likely a mistake so it is disallowed")
        }
        dd = -dd;
    } else if dir == "S" || dir == "W" {
        dd = -dd;
    }
    if dd > max_abs {
        return Err("angle cannot be greater than {max_abs:.1}");
    }
    if dd < -max_abs {
        return Err("angle cannot be less than than -{max_abs:.1}");
    }
    return Ok(dd);
}

pub fn parse_pos_string(dms: &str) -> Result<(f64, f64), &'static str> {
    // decimal degrees
    let regex_dd_part = r#"(-?)([\d]+(?:.[\d]+)?)[\s]*(?:°|d|deg|)?"#;
    let regex_dd = Regex::new(&format!(r#"^{}[\s]*(N|S|)[\s,]*{}[\s]*(E|W|)$"#, regex_dd_part, regex_dd_part)).expect("invalid regex pattern");
    if let Some(captures) = regex_dd.captures(dms) {
        let (_, [
            lat_sign, lat_str, lat_dir,
            long_sign, long_str, long_dir,
        ]) = captures.extract();
        let lat = dd_string_to_dd(lat_sign, lat_str, lat_dir, 90.0)?;
        let long = dd_string_to_dd(long_sign, long_str, long_dir, 180.0)?;
        return Ok((lat, long));
    }

    // degree, minutes
    let regex_dm_part = r#"(-?)([\d]+)[\s]*(?:°|d|deg)[\s]*([\d]+(?:.[\d]+)?)(?:'|\u2018|\u2019|m|min)"#;
    let regex_dm = Regex::new(&format!(r#"^{}[\s]*(N|S)[\s,]*{}[\s]*(E|W)$"#, regex_dm_part, regex_dm_part)).expect("invalid regex pattern");
    if let Some(captures) = regex_dm.captures(dms) {
        let (_, [
            lat_sign, lat_deg, lat_min, lat_dir,
            long_sign, long_deg, long_min, long_dir,
        ]) = captures.extract();
        let lat = dm_string_to_dd(lat_sign, lat_deg, lat_min, lat_dir, 90.0)?;
        let long = dm_string_to_dd(long_sign, long_deg, long_min, long_dir, 180.0)?;
        return Ok((lat, long))
    }

    // degree, minutes, seconds
    let regex_dms_part = r#"(-?)([\d]+)[\s]*(?:°|d|deg)[\s]*([\d]+)[\s]*(?:'|\u2018|\u2019|m|min)[\s]*([\d]+(?:.[\d]+)?)(?:"|\u201C|\u201D|s|sec)"#;
    let regex_dms = Regex::new(&format!(r#"^{}[\s]*(N|S)[\s,]*{}[\s]*(E|W)$"#, regex_dms_part, regex_dms_part)).expect("invalid regex pattern");
    if let Some(captures) = regex_dms.captures(dms) {
        let (_, [
            lat_sign, lat_deg, lat_min, lat_sec, lat_dir,
            long_sign, long_deg, long_min, long_sec, long_dir,
        ]) = captures.extract();
        let lat = dms_string_to_dd(lat_sign, lat_deg, lat_min, lat_sec, lat_dir, 90.0)?;
        let long = dms_string_to_dd(long_sign, long_deg, long_min, long_sec, long_dir, 180.0)?;
        return Ok((lat, long))
    }

    return Err("parse error, expected in format \"<deg>°<min>'<sec>\"<N|S>, <deg>°<min>'<sec>\"<E|W>\"");
}

pub fn parse_population_string(s: &str) -> Result<u16, &'static str> {
    if s == "0" {
        Ok(0_u16)
    } else {
        let regex = Regex::new(r"([\d]+).([\d]+)(?:e|E)\+?([\d]+)").expect("invalid regex pattern");
        match regex.captures(s) {
            Some(captures) => {
                let (_, [ int_str, dec_str, sig_str ]) = captures.extract();
                let integer = int_str.parse::<u16>().or(Err("whole part not a valid integer"))?;
                let decimal = dec_str.parse::<u16>().or(Err("decimal part not a valid integer"))?;
                let significand = sig_str.parse::<u16>().or(Err("significand not a valid integer"))?;
                
                if integer > 9 {
                    return Err("whole part must be a single digit, 0-9");
                }
                if decimal > 9 {
                    return Err("decimal part must be a single digit, 0-9");
                }
                if significand > 15 {
                    return Err("significand must be between 0-15 inclusive");
                }

                return Ok( ( ((integer & 0x0f) << 8) | ((decimal & 0x0f) << 4) | (significand & 0x0f) ) as u16 );
            }
            None => Err("parse error, expected in format \"<whole>.<decimal>e+<significand>\"")
        }
    }
}
pub fn format_population(pop: u16) -> String {
    if pop & 0x0ff0 == 0 {
        "0".to_owned()
    } else {
        format!("{}.{}e+{}", pop >> 8, pop >> 4 & 0x0f, pop & 0x0f)
    }
}

fn format_position(lat_dd: f64, long_dd: f64) -> String {
    format!("{}, {}", dd_to_dms(lat_dd, 'N', 'S'), dd_to_dms(long_dd, 'E', 'W'))
}

impl GeoDatabase {
    pub fn print_info(self: &GeoDatabase) {
        println!("Comment: {}", self.comment);
        println!("{} cities", self.cities.len());
        println!("{} countries", self.countries.len());
        println!("{} regions", self.regions.len());
        println!("{} subregions", self.subregions.len());
        println!("{} timezones", self.timezones.len());
        println!("{} features", self.features.len());
    }

    pub fn print_city_info(self: &Self, city_ix: usize) {
        let city = &self.cities[city_ix];
        println!("{}, {}, {}, {}", city.name, self.subregions[city.subregion_ix], self.regions[city.region_ix], self.country_name(city.country_ix));
        println!("{}", format_position(city.latitude, city.longitude));
        println!("Timezone: {}, Population: {}", self.timezones[city.timezone_ix], format_population(city.population));
        println!("{}", self.features[city.feature_ix]);
    }

    pub fn print_subregion_info(self: &Self, subregion_ix: usize) {
        let ( region_ix, country_ix, _ ) = self.subregion_parents(subregion_ix);
        let mut n_cities: u32 = 0;
        let mut timezones = HashSet::new();
        for city in &self.cities {
            if city.subregion_ix == subregion_ix {
                n_cities += 1;
                timezones.insert(city.timezone_ix);
            }
        }

        println!("{}, {}, {}", self.subregions[subregion_ix], self.regions[region_ix], self.country_name(country_ix));
        println!("Containing {} {}", n_cities, if n_cities == 1 { "city" } else { "cities" });
        println!("Covers {} {}", timezones.len(), if timezones.len() == 1 { "timezone" } else { "timezones" });
    }

    pub fn print_region_info(self: &Self, region_ix: usize) {
        let country_ix = self.region_parent(region_ix);
        let mut n_cities: u32 = 0;
        let mut subregions = HashSet::new();
        let mut timezones = HashSet::new();
        for city in &self.cities {
            if city.region_ix == region_ix {
                n_cities += 1;
                subregions.insert(city.subregion_ix);
                timezones.insert(city.timezone_ix);
            }
        }

        println!("{}, {}", self.regions[region_ix], self.country_name(country_ix));
        println!("Containing {} {}", n_cities, if n_cities == 1 { "city" } else { "cities" });
        println!("Containing {} sub-region{}", subregions.len(), if subregions.len() == 1 { "" } else { "s" });
        println!("Covers {} {}", timezones.len(), if timezones.len() == 1 { "timezone" } else { "timezones" });
    }

    pub fn print_country_info(self: &Self, country_ix: usize) {
        let mut n_cities: u32 = 0;
        let mut subregions = HashSet::new();
        let mut regions = HashSet::new();
        let mut timezones = HashSet::new();
        for city in &self.cities {
            if city.country_ix == country_ix {
                n_cities += 1;
                subregions.insert(city.subregion_ix);
                regions.insert(city.region_ix);
                timezones.insert(city.timezone_ix);
            }
        }

        println!("{} ({})", self.country_name(country_ix), self.country_code(country_ix));
        println!("Containing {} {}", n_cities, if n_cities == 1 { "city" } else { "cities" });
        println!("Containing {} sub-region{}", subregions.len(), if subregions.len() == 1 { "" } else { "s" });
        println!("Containing {} region{}", regions.len(), if regions.len() == 1 { "" } else { "s" });
        println!("Covers {} {}", timezones.len(), if timezones.len() == 1 { "timezone" } else { "timezones" });
    }

    pub fn find_matching_cities(self: &GeoDatabase, name: &str) -> Vec<usize> {
        let name_parts: Vec<&str> = name.split(',').collect();
        let (name, subregion, region, country) = match name_parts.len() {
            1 => (name_parts[0], None, None, None),
            2 => (name_parts[0].trim(), None, None, Some(name_parts[1].trim())),
            3 => (name_parts[0].trim(), None, Some(name_parts[1].trim()), Some(name_parts[2].trim())),
            4 => (name_parts[0].trim(), Some(name_parts[1].trim()), Some(name_parts[2].trim()), Some(name_parts[3].trim())),
            _ => panic!("Cannot have more than 4 parts to a city search string")
        };

        return self.cities.iter().enumerate()
            .filter(|(_, city)| {
                return city.name == name && match country {
                    None => true,
                    Some(country) => self.countries[city.country_ix].contains(country)
                } && match region {
                    None => true,
                    Some(region) => self.regions[city.region_ix].contains(region)
                } && match subregion {
                    None => true,
                    Some(subregion) => self.subregions[city.subregion_ix].contains(subregion)
                };
            })
            .map(|(city_ix, _)| city_ix)
            .collect();
    }

    pub fn add_city(self: &mut Self, city: CityEntry) {
        self.cities.push(city);
    }

    pub fn remove_city(self: &mut Self, city_ix: usize) {
        self.cities.remove(city_ix);
    }

    pub fn print_matching_cities(self: &GeoDatabase, name: &str, max_displayed: usize) {
        print_entries(
            self.find_matching_cities(name), 
            |city| self.print_city_info(city),
            max_displayed
        );
    }

    pub fn print_cities(self: &GeoDatabase) {
        self.cities.iter().for_each(|city| println!("{}", city.name));
    }

    pub fn find_matching_subregions(self: &GeoDatabase, name: &str) -> Vec<usize> {
        let name_parts: Vec<&str> = name.split(',').collect();
        let (name, region, country) = match name_parts.len() {
            1 => (name_parts[0], None, None),
            2 => (name_parts[0].trim(), None, Some(name_parts[1].trim())),
            3 => (name_parts[0].trim(), Some(name_parts[1].trim()), Some(name_parts[2].trim())),
            _ => panic!("Cannot have more than 3 parts to a subregion search string")
        };

        self.subregions.iter().enumerate()
            .filter(|(subregion_ix, subregion)| {
                if *subregion == name {
                    let ( region_ix, country_ix, _ ) = self.subregion_parents(*subregion_ix);
                    return match country {
                        None => true,
                        Some(country) => self.countries[country_ix].contains(country)
                    } && match region {
                        None => true,
                        Some(region) => self.regions[region_ix].contains(region)
                    };
                }
                return false;
            })
            .map(|(subregion_ix, _)| {
                subregion_ix
            })
            .collect()
    }

    pub fn print_matching_subregion(self: &GeoDatabase, name: &str, max_displayed: usize) {
        print_entries(
            self.find_matching_subregions(name), 
            |ix| self.print_subregion_info(ix),
            max_displayed
        );
    }
    
    pub fn print_subregions(self: &GeoDatabase) {
        self.subregions.iter().for_each(|subregion| println!("{}", subregion));
    }

    pub fn find_matching_regions(self: &GeoDatabase, name: &str) -> Vec<usize> {
        let name_parts: Vec<&str> = name.split(',').collect();
        let (name, country) = match name_parts.len() {
            1 => (name_parts[0], None),
            2 => (name_parts[0].trim(), Some(name_parts[1].trim())),
            _ => panic!("Cannot have more than 2 parts to a region search string")
        };

        self.regions.iter().enumerate()
            .filter(|(region_ix, region)| {
                if *region == name {
                    let country_ix = self.region_parent(*region_ix);
                    return match country {
                        None => true,
                        Some(country) => self.countries[country_ix].contains(country)
                    };
                }
                return false;
            })
            .map(|(region_ix, _)| {
                region_ix
            })
            .collect()
    }

    pub fn print_matching_regions(self: &GeoDatabase, name: &str, max_displayed: usize) {
        print_entries(
            self.find_matching_regions(name), 
            |ix| self.print_region_info(ix),
            max_displayed
        );
    }
    
    pub fn print_regions(self: &GeoDatabase) {
        self.regions.iter().for_each(|region| println!("{}", region));
    }

    pub fn find_matching_countries(self: &GeoDatabase, name: &str) -> Vec<usize> {
        self.countries.iter().enumerate()
            .filter(|(_, country)| {
                return country.contains(name);
            })
            .map(|(country_ix, _)| {
                country_ix
            })
            .collect()
    }

    pub fn print_matching_country(self: &GeoDatabase, name: &str, max_displayed: usize) {
        print_entries(
            self.find_matching_countries(name), 
            |ix| self.print_country_info(ix),
            max_displayed
        );
    }
    
    pub fn print_countires(self: &GeoDatabase) {
        self.regions.iter().enumerate().for_each(|(country_ix, _)| println!("{}", self.country_name(country_ix)));
    }
    
    pub fn find_matching_timezones(self: &GeoDatabase, name: &str) -> Vec<usize> {
        self.timezones.iter().enumerate()
            .filter(|(_, timezone)| {
                return timezone.starts_with(name);
            })
            .map(|(timezone_ix, _)| {
                timezone_ix
            })
            .collect()
    }

    pub fn print_timezones(self: &GeoDatabase) {
        self.timezones.iter().for_each(|timezone, | println!("{}", timezone));
    }
    
    pub fn find_matching_features(self: &GeoDatabase, name: &str) -> Vec<usize> {
        self.features.iter().enumerate()
            .filter(|(_, feature)| {
                return feature.contains(name);
            })
            .map(|(feature_ix, _)| {
                feature_ix
            })
            .collect()
    }
    
    pub fn print_features(self: &GeoDatabase) {
        self.features.iter().for_each(|feature, | println!("{}", feature));
    }

    /// Find the region, country and timezone which contain this sub-region
    pub fn subregion_parents(self: &Self, subregion_ix: usize) -> ( usize, usize, usize ) {
        for city in &self.cities {
            if city.subregion_ix == subregion_ix {
                return ( city.region_ix, city.country_ix, city.timezone_ix );
            }
        }
        panic!("Didn't find any cities in this subregion");
    }

    pub fn region_parent(self: &Self, region_ix: usize) -> usize {
        for city in &self.cities {
            if city.region_ix == region_ix {
                return city.country_ix;
            }
        }
        panic!("Didn't find any cities in this region");
    }

    pub fn subregion_name<'a>(self: &'a Self, subregion_ix: usize) -> &'a str {
        return &self.subregions[subregion_ix];
    }

    pub fn region_name<'a>(self: &'a Self, region_ix: usize) -> &'a str {
        return &self.regions[region_ix];
    }

    pub fn country_name<'a>(self: &'a GeoDatabase, country_ix: usize) -> &'a str {
        return &self.countries[country_ix][2..];
    }

    pub fn country_code<'a>(self: &'a GeoDatabase, country_ix: usize) -> &'a str {
        return &self.countries[country_ix][0..2];
    }

    pub fn timezone_name<'a>(self: &'a Self, timezone_ix: usize) -> &'a str {
        return &self.timezones[timezone_ix];
    }

    pub fn feature_name<'a>(self: &'a Self, feature_ix: usize) -> &'a str {
        return &self.features[feature_ix];
    }

    pub fn read_from(reader: &mut dyn BufRead) -> Result<GeoDatabase, DatabaseReadError> {
        let header_line = read_line(reader)?;
        let comment = read_line(reader)?;
    
        let version_string = parse_header(&header_line)?;
        if version_string != "1.03" {
            return Err(DatabaseReadError::UnsupportedVersion { expected: String::from("1.03"), found: String::from(version_string) });
        }
    
        let mut buf = [0; 13];
    
        // cities
        let mut cities = vec![];
        loop {
            let (buf_start, buf_end) = buf.split_at_mut(6);
            reader.read_exact(buf_start)?;
            if buf_start == [0,0,0,0,1,0xA] {
                break;
            }
            reader.read_exact(buf_end)?;
    
            let city = parse_city_entry(&buf, reader)?;
            cities.push(city);
        }
    
        // countries
        let mut countries = vec![];
        loop {
            let country = read_line(reader)?;
            if country.as_bytes() == [0,0,0,0,2] {
                break;
            }
    
            countries.push(country);
        }
    
        // regions
        let mut regions = vec![];
        loop {
            let region = read_line(reader)?;
            if region.as_bytes() == [0,0,0,0,3] {
                break;
            }
    
            regions.push(region);
        }
    
        // subregions
        let mut subregions = vec![];
        loop {
            let subregion = read_line(reader)?;
            if subregion.as_bytes() == [0,0,0,0,4] {
                break;
            }
    
            subregions.push(subregion);
        }
    
        // timezones
        let mut timezones = vec![];
        loop {
            let timezone = read_line(reader)?;
            if timezone.as_bytes() == [0,0,0,0,5] {
                break;
            }
    
            timezones.push(timezone);
        }
    
        // features
        let mut features = vec![];
        loop {
            let feature = read_line(reader)?;
            if feature.as_bytes() == [0,0,0,0,0] {
                break;
            }
    
            features.push(feature);
        }
    
        Ok(GeoDatabase{
            comment, cities, countries, regions, subregions, timezones, features
        })
    }

    pub fn write_to(self: &Self, writer: &mut dyn Write) -> Result<(), io::Error> {
        writeln!(writer, "Geolocation1.03 {}", self.cities.len())?;
        writeln!(writer, "{}", self.comment)?;
    
        // cities
        for city in &self.cities {
            write_city_entry(writer, city)?;
        }
        writer.write_all(&[0, 0, 0, 0, 1, 0xA])?;
    
        // countries
        for country in &self.countries {
            writeln!(writer, "{}", country)?;
        }
        writer.write_all(&[0, 0, 0, 0, 2, 0xA])?;
    
        // regions
        for region in &self.regions {
            writeln!(writer, "{}", region)?;
        }
        writer.write_all(&[0, 0, 0, 0, 3, 0xA])?;
    
        // subregions
        for subregion in &self.subregions {
            writeln!(writer, "{}", subregion)?;
        }
        writer.write_all(&[0, 0, 0, 0, 4, 0xA])?;
    
        // timezones
        for timezone in &self.timezones {
            writeln!(writer, "{}", timezone)?;
        }
        writer.write_all(&[0, 0, 0, 0, 5, 0xA])?;
    
        // features
        for feature in &self.features {
            writeln!(writer, "{}", feature)?;
        }
        writer.write_all(&[0, 0, 0, 0, 0, 0xA])?;
    
        Ok(())
    }
}

fn print_entries<T, F>(entries: Vec<T>, display: F, max_displayed: usize)
where
    T: Copy,
    F: Fn(T) -> ()
{
    println!("-----------------------");
    for entry in entries.iter().take(max_displayed) {
        display(*entry);
        println!("-----------------------");
    }
    if entries.len() == 0 {
        println!("No results");
        println!("-----------------------");
    } else if entries.len() > max_displayed {
        println!("     and {} more", entries.len() - max_displayed);
        println!("-----------------------");
    }
}

fn parse_header<'a>(header: &'a str) -> Result<&'a str, DatabaseReadError> {
    let header_regex = Regex::new(r"Geolocation([\d]+.[\d]+)[\s]+([\d]+)").expect("invalid regex pattern");
    return match header_regex.captures(header) {
        Some(captures) => {
            let (_, [version, _n_cities]) = captures.extract();
            Ok(version)
        }
        None => Err(DatabaseReadError::InvalidHeader { msg: String::from("Expected \"Geolocation x.xx (n)\" where \"x.xx\" is the database version number and \"n\" is the number of cities in the database") })
    };
}

pub enum DatabaseReadError {
    UnsupportedVersion{ expected: String, found: String },
    InvalidHeader{ msg: String },
    IoError{ source: io::Error }
}

impl From<io::Error> for DatabaseReadError {
    fn from(value: io::Error) -> Self {
        DatabaseReadError::IoError { source: value }
    }
}