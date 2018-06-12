mod metadata;
mod ar;
mod deb;
mod debian;
mod html_links;

use std::io::{stdout, stderr, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

use failure::{Error, err_msg};
use regex::Regex;
use argparse::{ArgumentParser, Parse, Collect, StoreConst};

use config::{Config, RepositoryType};
use repo::metadata::gather_metadata;


#[derive(Debug, Clone, Copy)]
pub enum ConflictResolution {
    Error,
    Keep,
    Replace,
}


fn _repo_add(config: &Path, packages: &Vec<String>, dir: &Path,
    on_conflict: ConflictResolution)
    -> Result<(), Error>
{
    let packages = packages.iter().map(gather_metadata)
        .collect::<Result<Vec<_>, _>>()?;
    debug!("Packages read {:#?}", packages);
    let cfg = Config::parse_file(&config)
        .map_err(|e| format_err!("can't parse config {:?}: {}", config, e))?;
    let mut debian = debian::Repository::new(dir);
    let mut html_links = html_links::Repository::new(dir);

    for repo in &cfg.repositories {
        let version_re = match repo.match_version {
            Some(ref re) => Some(Regex::new(re)?),
            None => None,
        };
        let skip_re = match repo.skip_version {
            Some(ref re) => Some(Regex::new(re)?),
            None => None,
        };
        let matching = packages.iter()
            .filter(|p| {
                version_re.as_ref().map(|x| x.is_match(&p.version))
                .unwrap_or(true) &&
                !skip_re.as_ref().map(|x| x.is_match(&p.version))
                .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        if matching.len() > 0 {
            match repo.kind {
                RepositoryType::Debian => {
                    let (suite, comp) = match (&repo.suite, &repo.component) {
                        (&Some(ref suite), &Some(ref comp)) => (suite, comp),
                        _ => {
                            return Err(err_msg("Debian repository requires \
                                    suite and component to be specified"));
                        }
                    };
                    for p in matching {
                        debian.open(&repo, &p.arch)?
                            .add_package(p, on_conflict)?;
                        if repo.add_empty_i386_repo && p.arch != "i386" {
                            debian.open(&repo, "i386")?;
                        }
                    }
                }
                RepositoryType::HtmlLinks => {
                    for p in matching {
                        let empty_path = PathBuf::new();
                        let index = repo.index.as_ref().unwrap_or(&empty_path);
                        let files = repo.files.as_ref().unwrap_or(&empty_path);
                        html_links.open(index, files)?
                            .add_package(p, on_conflict)?;
                    }
                }
            }
        }
    }
    debian.write()?;
    html_links.write()?;
    Ok(())
}


pub fn repo_add(args: Vec<String>) {
    let mut config = PathBuf::from("bulk.yaml");
    let mut repo_dir = PathBuf::new();
    let mut packages = Vec::<String>::new();
    let mut conflict = ConflictResolution::Error;
    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut config)
            .add_option(&["-c", "--config"], Parse,
                "Package configuration file");
        ap.refer(&mut repo_dir)
            .add_option(&["-D", "--repository-base"], Parse,
                "Directory where repositories are stored");
        ap.refer(&mut conflict)
            .add_option(&["--skip-existing"],
                StoreConst(ConflictResolution::Keep),
                "Skip package if it's already in the repository")
            .add_option(&["--replace-existing"],
                StoreConst(ConflictResolution::Replace),
                "Replace package if it's already in the repository");
        ap.refer(&mut packages)
            .add_argument("packages", Collect,
                "Package file names to add");
        match ap.parse(args, &mut stdout(), &mut stderr()) {
            Ok(()) => {}
            Err(x) => exit(x),
        }
    }

    match _repo_add(&config, &packages, &repo_dir, conflict) {
        Ok(()) => {}
        Err(err) => {
            writeln!(&mut stderr(), "Error: {}", err).ok();
            exit(1);
        }
    }
}
