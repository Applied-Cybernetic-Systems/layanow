//! Print the platform's current native text selection (M4 smoke check).
//!
//! ```sh
//! printf 'layassist primary probe' | wl-copy -p
//! cargo run -p layassist-platform --example read_primary
//! wl-copy -c -p
//! ```

use layassist_resolvers::TextResolver;

fn main() {
    let resolver: Box<dyn TextResolver> = layassist_platform::selection::resolver();
    eprintln!("resolver: {} (available: {})", resolver.name(), resolver.available());
    match resolver.resolve_current_selection() {
        Ok(Some(selection)) => println!("{}", selection.text),
        Ok(None) => eprintln!("no selection"),
        Err(error) => eprintln!("error: {error}"),
    }
}
