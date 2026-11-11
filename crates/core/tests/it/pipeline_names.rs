//! The file name a recipe's name gives its files.

use bullterrier_image::pipeline::{file_name, safe_name};
use bullterrier_image::{Error, Recipe};

#[test]
fn a_name_is_made_safe_run_by_run() {
    let rows = [
        ("my look", "my-look"),
        ("my warm look!", "my-warm-look"),
        ("-a--b-", "a-b"),
        ("..hidden", "hidden"),
        ("a.b.", "a.b"),
        ("日本", ""),
        ("con", ""),
        ("CON.txt", ""),
        ("com1", ""),
        ("com10", "com10"),
        ("", ""),
        ("!!!", ""),
        ("a/../b", "a-..-b"),
    ];
    for (text, safe) in rows {
        assert_eq!(safe_name(text), safe, "{text:?}");
    }
    assert_eq!(safe_name(&"a".repeat(100)).len(), 64);
    assert_eq!(
        safe_name(&format!("{}!", "a".repeat(63))),
        "a".repeat(63),
        "cut to 64, then a trailing dash goes"
    );
}

#[test]
fn a_recipe_without_a_usable_name_is_refused_naming_the_fix() {
    let none = file_name(&Recipe::default());
    assert!(
        matches!(&none, Err(Error::Recipe(message)) if message.contains("name=")),
        "{none:?}"
    );
    let recipe = Recipe {
        name: Some("日本".to_owned()),
        ..Recipe::default()
    };
    let unusable = file_name(&recipe);
    assert!(
        matches!(&unusable, Err(Error::Recipe(message)) if message.contains("'日本'") && message.contains("name=")),
        "{unusable:?}"
    );
    let named = Recipe {
        name: Some("my look".to_owned()),
        ..Recipe::default()
    };
    assert_eq!(file_name(&named).ok().as_deref(), Some("my-look"));
}
