//! Reading a `.cube` file: what the Adobe/Resolve format allows is read, every malformed input is refused naming the
//! file and the number of the faulty line (never its text), and a file is read through a limit.

use bullterrier_image::{Error, ErrorKind, Lut};

use crate::common::files::Scratch;

/// The rows of a 2 x 2 x 2 identity table, red changing fastest.
const ROWS: &str = "0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

fn parse(text: &str) -> Result<Lut, Error> {
    Lut::from_cube(text, "film.cube")
}

fn identity() -> Result<Lut, Error> {
    Lut::identity(2)
}

#[test]
fn the_rows_are_the_table_red_changing_fastest() -> Result<(), Error> {
    assert_eq!(parse(&format!("LUT_3D_SIZE 2\n{ROWS}"))?, identity()?);
    // row 1 (second) is one step in red: giving it an unmistakable value moves only the red corner
    let marked = ROWS.replacen("1 0 0", "0.25 0.5 0.75", 1);
    let table = parse(&format!("LUT_3D_SIZE 2\n{marked}"))?;
    assert_eq!(table.lookup([1.0, 0.0, 0.0]), [0.25, 0.5, 0.75]);
    assert_eq!(table.lookup([0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
    Ok(())
}

#[test]
fn what_the_format_allows_is_read_the_same() -> Result<(), Error> {
    let expected = identity()?;
    let variants = [
        (
            "comments and a title",
            format!("TITLE \"a look\"\n# made by hand\nLUT_3D_SIZE 2 # two\n{ROWS}# end\n"),
        ),
        (
            "blank lines and indentation",
            format!("\n\n  LUT_3D_SIZE   2  \n\n\t{ROWS}\n"),
        ),
        (
            "windows line ends",
            format!("LUT_3D_SIZE 2\r\n{}", ROWS.replace('\n', "\r\n")),
        ),
        (
            "old Mac line ends",
            format!("LUT_3D_SIZE 2\r{}", ROWS.replace('\n', "\r")),
        ),
        (
            "the default domain written",
            format!("DOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\nLUT_3D_SIZE 2\n{ROWS}"),
        ),
        (
            "the domain after the size",
            format!("LUT_3D_SIZE 2\nDOMAIN_MAX 1 1 1\nDOMAIN_MIN 0 0 0\n{ROWS}"),
        ),
        (
            "one range for all channels",
            format!("LUT_3D_INPUT_RANGE 0.0 1.0\nLUT_3D_SIZE 2\n{ROWS}"),
        ),
        (
            "a title with numbers in it",
            format!("TITLE 1 2 3 not a row\nLUT_3D_SIZE 2\n{ROWS}"),
        ),
        (
            "numbers as they may be written",
            "LUT_3D_SIZE 2.0\n+0.0 .0 0.\n+1 0 0\n0 1e0 0\n1 1 0\n0 0 1.0\n1 0 1\n0 1 1\n1 1 1\n".to_owned(),
        ),
        (
            "a second size replacing the first",
            format!("LUT_3D_SIZE 3\nLUT_3D_SIZE 2\n{ROWS}"),
        ),
        (
            "a last line without its end",
            format!("LUT_3D_SIZE 2\n{}", ROWS.trim_end()),
        ),
    ];
    for (what, text) in variants {
        assert_eq!(parse(&text)?, expected, "{what}");
    }
    Ok(())
}

#[test]
fn the_domain_keywords_set_the_domain_in_the_order_they_are_written() -> Result<(), Error> {
    let domain = |head: &str| -> Result<([f32; 3], [f32; 3]), Error> {
        let table = parse(&format!("{head}\nLUT_3D_SIZE 2\n{ROWS}"))?;
        Ok((table.domain_min(), table.domain_max()))
    };
    assert_eq!(
        domain("DOMAIN_MIN -1 -2 -3\nDOMAIN_MAX 1 2 3")?,
        ([-1.0, -2.0, -3.0], [1.0, 2.0, 3.0])
    );
    assert_eq!(domain("LUT_3D_INPUT_RANGE -0.5 4")?, ([-0.5; 3], [4.0; 3]));
    // a later keyword replaces what an earlier one set
    assert_eq!(
        domain("LUT_3D_INPUT_RANGE 0 4\nDOMAIN_MAX 1 2 3")?,
        ([0.0; 3], [1.0, 2.0, 3.0])
    );
    assert_eq!(
        domain("DOMAIN_MAX 1 2 3\nLUT_3D_INPUT_RANGE 0 4")?,
        ([0.0; 3], [4.0; 3])
    );
    Ok(())
}

/// Each text with a faulty line, with the words its message must have: the file and the line.
fn bad_texts() -> Vec<(&'static str, String, Vec<&'static str>)> {
    let with_size = |tail: &str| format!("LUT_3D_SIZE 2\n{tail}");
    vec![
        ("1D table", "LUT_1D_SIZE 4\n0 0 0\n".to_owned(), vec!["line 1", "1D"]),
        (
            "no size",
            format!("TITLE \"x\"\n{ROWS}"),
            vec!["line 2", "before LUT_3D_SIZE"],
        ),
        (
            "size 1",
            "LUT_3D_SIZE 1\n0 0 0\n".to_owned(),
            vec!["line 1", "LUT_3D_SIZE"],
        ),
        (
            "size 257",
            "LUT_3D_SIZE 257\n".to_owned(),
            vec!["line 1", "LUT_3D_SIZE", "257"],
        ),
        (
            "size not whole",
            "LUT_3D_SIZE 2.5\n".to_owned(),
            vec!["line 1", "LUT_3D_SIZE"],
        ),
        (
            "size negative",
            "LUT_3D_SIZE -2\n".to_owned(),
            vec!["line 1", "LUT_3D_SIZE"],
        ),
        (
            "size nan",
            "LUT_3D_SIZE nan\n".to_owned(),
            vec!["line 1", "LUT_3D_SIZE"],
        ),
        (
            "size and more",
            "LUT_3D_SIZE 2 2\n".to_owned(),
            vec!["line 1", "needs 1 numbers, got 2"],
        ),
        (
            "too few rows",
            with_size(&ROWS[..ROWS.len() - 6]),
            vec!["needs 8 rows", "has 7"],
        ),
        (
            "too many rows",
            with_size(&format!("{ROWS}0 0 0\n")),
            vec!["needs 8 rows", "has 9"],
        ),
        (
            "a row of two numbers",
            with_size("0 0\n"),
            vec!["line 2", "needs 3 numbers, got 2"],
        ),
        (
            "a row of four numbers",
            with_size("0 0 0 0\n"),
            vec!["line 2", "needs 3 numbers, got 4"],
        ),
        ("a word in a row", with_size("0 0 x\n"), vec!["line 2", "cannot read"]),
        (
            "a word after a keyword",
            "DOMAIN_MIN 0 0 x\nLUT_3D_SIZE 2\n".to_owned(),
            vec!["line 1", "cannot read"],
        ),
        (
            "a domain of two numbers",
            "DOMAIN_MIN 0 0\n".to_owned(),
            vec!["line 1", "DOMAIN_MIN needs 3 numbers, got 2"],
        ),
        (
            "a range of one number",
            "LUT_3D_INPUT_RANGE 0\n".to_owned(),
            vec!["line 1", "needs 2 numbers"],
        ),
        (
            "unknown keyword",
            with_size("GAMMA 2.2\n"),
            vec!["line 2", "unknown keyword"],
        ),
        (
            "a keyword in lower case",
            "lut_3d_size 2\n".to_owned(),
            vec!["line 1", "unknown keyword"],
        ),
    ]
}

/// Each text that is malformed as a whole, not at a line.
fn bad_files() -> Vec<(&'static str, String, Vec<&'static str>)> {
    vec![
        (
            "domain upside down",
            format!("DOMAIN_MIN 1 1 1\nDOMAIN_MAX 0 0 0\nLUT_3D_SIZE 2\n{ROWS}"),
            vec!["domain"],
        ),
        (
            "infinite value",
            format!("LUT_3D_SIZE 2\n{}", ROWS.replacen("0 0 0", "1e999 0 0", 1)),
            vec!["finite"],
        ),
        (
            "infinite domain",
            format!("DOMAIN_MAX 1e999 1 1\nLUT_3D_SIZE 2\n{ROWS}"),
            vec!["finite"],
        ),
        ("empty file", String::new(), vec!["no LUT_3D_SIZE"]),
        ("only comments", "# nothing here\n".to_owned(), vec!["no LUT_3D_SIZE"]),
    ]
}

#[test]
fn a_cube_that_cannot_be_used_is_refused_naming_the_file_and_the_line() {
    for (what, text, named) in bad_texts().into_iter().chain(bad_files()) {
        let error = parse(&text).err();
        assert!(
            error.as_ref().is_some_and(|error| {
                error.kind() == ErrorKind::Lut
                    && error.to_string().contains("film.cube")
                    && named.iter().all(|word| error.to_string().contains(word))
            }),
            "{what}: {error:?}"
        );
    }
}

#[test]
fn a_fault_names_the_line_number_and_never_quotes_a_line_of_the_file() {
    for line in [
        "127.0.0.1\tsecret-host",
        "root:secret:0:0:/var/root",
        "3 secret 5",
        "SECRET 1 2 3",
        "1 2",
    ] {
        let message = parse(&format!("LUT_3D_SIZE 2\n{line}\n"))
            .err()
            .map(|error| error.to_string());
        assert!(
            message.as_deref().is_some_and(|text| text.contains("line 2")
                && !text.to_lowercase().contains("secret")
                && !text.contains(line)),
            "{line:?}: {message:?}"
        );
    }
}

#[test]
fn the_line_numbers_count_every_line_ending() {
    // lines: 1 size, 2 blank, 3 comment, 4 the fault (a lone carriage return and a windows ending count as one each)
    let message = parse("LUT_3D_SIZE 2\r\n\r# note\r\nWHAT 1\n")
        .err()
        .map(|error| error.to_string());
    assert!(
        message.as_deref().is_some_and(|text| text.contains("line 4")),
        "{message:?}"
    );
}

#[test]
fn a_cube_file_is_read_and_labelled_by_the_path_as_given() -> Result<(), Box<dyn std::error::Error>> {
    let folder = Scratch::new()?;
    let good = folder.write("film.cube", format!("LUT_3D_SIZE 2\n{ROWS}"))?;
    assert_eq!(Lut::from_cube_file(&good)?, identity()?);
    let bad = folder.write("bad.cube", "LUT_3D_SIZE 2\nWHAT 1\n")?;
    let message = Lut::from_cube_file(&bad).err().map(|error| error.to_string());
    assert!(
        message
            .as_deref()
            .is_some_and(|text| text.contains(&bad.display().to_string()) && text.contains("line 2")),
        "{message:?}"
    );
    Ok(())
}

#[test]
fn a_file_that_is_missing_a_folder_or_not_text_is_refused_naming_it() -> Result<(), Box<dyn std::error::Error>> {
    let folder = Scratch::new()?;
    let binary = folder.write("binary.cube", [0xff_u8, 0xfe, 0x00, 0x01])?;
    for path in [folder.path("missing.cube"), binary, folder.path("")] {
        let error = Lut::from_cube_file(&path).err();
        assert!(
            error.as_ref().is_some_and(
                |error| error.kind() == ErrorKind::Lut && error.to_string().contains(&path.display().to_string())
            ),
            "{path:?}: {error:?}"
        );
    }
    Ok(())
}

/// The most bytes of a file that are read.
const LIMIT: usize = 64 * 1024 * 1024;

#[test]
fn a_file_at_the_limit_is_read_and_one_byte_over_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let folder = Scratch::new()?;
    let head = format!("LUT_3D_SIZE 2\n{ROWS}");
    for (size, refused) in [(LIMIT, false), (LIMIT + 1, true)] {
        // the rest of the file is one long comment
        let path = folder.write("big.cube", format!("{head}{}", "#".repeat(size - head.len())))?;
        let made = Lut::from_cube_file(&path);
        if refused {
            let message = made.err().map(|error| error.to_string());
            assert!(
                message.as_deref().is_some_and(|text| text.contains("64 MiB")),
                "{message:?}"
            );
        } else {
            assert_eq!(made?.size(), 2);
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_file_that_never_ends_is_refused_after_reading_only_the_limit() {
    let message = Lut::from_cube_file(std::path::Path::new("/dev/zero"))
        .err()
        .map(|error| error.to_string());
    assert!(
        message.as_deref().is_some_and(|text| text.contains("64 MiB")),
        "{message:?}"
    );
}
