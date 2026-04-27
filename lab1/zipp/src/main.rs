use::std::fs::File;
use::zip::ZipArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("test.zip")?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        println!("{}", file.name());
    }

    Ok(())
}