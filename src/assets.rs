use speedy2d::font::Font;
use std::cell::RefCell;
use std::collections::HashMap;

pub trait AssetsProvider {
    fn get_file(&self, path: &str) -> Option<&[u8]>;
}

thread_local! {
    static PROVIDER: RefCell<Option<Box<dyn AssetsProvider>>> = RefCell::new(None);
    static FONTS: RefCell<HashMap<(String, String), Font>> = RefCell::new(HashMap::new());
}

pub fn set_provider(value: Box<impl AssetsProvider + 'static>) {
    PROVIDER.with(|cell| {
        *cell.borrow_mut() = Some(value);
    });
}

pub fn get_font(name: &str, style: &str) -> Option<Font> {
    let mut result = None;
    PROVIDER.with(|provider| {
        match provider.borrow().as_ref() {
            None => {},
            Some(p) => {
                let key = (name.replace(" ", ""), style.replace(" ", ""));

                FONTS.with(|fonts| {
                    if !fonts.borrow().contains_key(&key) {
                        let separator = std::path::MAIN_SEPARATOR;
                        if let Some(bytes) = p.get_file(&format!("fonts{}{}-{}.ttf", separator, &key.0, &key.1)) {
                            match Font::new(bytes) {
                                Ok(font) => {
                                    fonts.borrow_mut().insert(key.clone(), font);
                                }
                                Err(_) => {
                                    println!("Error parsing font file from assets!");
                                }
                            }
                        }
                    }
                    result = fonts.borrow().get(&key).cloned()
                })
            }
        }
    });

    result
}