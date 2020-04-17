mod dichotomy_2d;

use crate::dezoomer::{
    single_level,
    Dezoomer, DezoomerError, DezoomerInput,
    TileFetchResult, TileProvider, TileReference,
    ZoomLevels,
};
use crate::Vec2d;

use std::collections::HashSet;

use lazy_static::lazy_static;
use regex::Regex;

/// A dezoomer that takes an image tile URL template like
/// `http://example.com/image_{{X}}_{{Y}}.jpg`
/// and automatically figures out the dimensions of the image.
#[derive(Default)]
pub struct GenericDezoomer;

impl Dezoomer for GenericDezoomer {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn zoom_levels(&mut self, data: &DezoomerInput) -> Result<ZoomLevels, DezoomerError> {
        self.assert(TEMPLATE_RE.is_match(&data.uri))?;
        let dezoomer = ZoomLevel {
            url_template: data.uri.clone(),
            dichotomy: Default::default(),
            last_tile: (1, 1),
            done: HashSet::new(),
            tile_size: None,
        };
        single_level(dezoomer)
    }
}

lazy_static! {
    static ref TEMPLATE_RE: Regex = Regex::new(r"(?xi)
    \{\{
        (?P<dimension>x|y)
        (?::0(?P<zeroes>\d+))?
     \}\}
    ").unwrap();
}

struct ZoomLevel {
    url_template: String,
    dichotomy: dichotomy_2d::Dichotomy2d,
    last_tile: (u32, u32),
    tile_size: Option<Vec2d>,
    done: HashSet<(u32, u32)>,
}

impl ZoomLevel {
    fn tile_url_at(&self, x: u32, y: u32) -> String {
        TEMPLATE_RE.replace_all(&self.url_template, |caps: &regex::Captures| {
            let dimension = caps.name("dimension")
                .expect("missing dimension")
                .as_str()
                .chars().next().expect("empty dim")
                .to_ascii_lowercase();
            let num = match dimension {
                'x' => x,
                'y' => y,
                _ => unreachable!("The dimension is either x or y")
            };
            let padding: usize = caps.name("zeroes")
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            format!("{num:0padding$}", num = num, padding = padding)
        }).to_string()
    }
    fn tile_ref_at(&self, x: u32, y: u32) -> TileReference {
        let tile_size = self.tile_size.unwrap_or(Vec2d { x: 0, y: 0 });
        let position = Vec2d { x, y } * tile_size;
        TileReference {
            url: self.tile_url_at(x, y),
            position,
        }
    }
}

impl TileProvider for ZoomLevel {
    fn next_tiles(&mut self, previous: Option<TileFetchResult>) -> Vec<TileReference> {
        if let Some(p) = previous {
            self.tile_size = self.tile_size.or(p.tile_size);
            if let Some((x, y)) = self.dichotomy.next(p.is_success()) {
                self.last_tile = (x, y);
                self.done.insert((x, y));
                vec![self.tile_ref_at(x, y)]
            } else if !self.done.is_empty() {
                let mut res = vec![];
                for y in 0..=self.last_tile.1 {
                    for x in 0..=self.last_tile.0 {
                        if !self.done.contains(&(x, y)) {
                            res.push(self.tile_ref_at(x, y));
                        }
                    }
                }
                self.done.clear();
                res
            } else {
                vec![]
            }
        } else {
            vec![self.tile_ref_at(self.last_tile.0, self.last_tile.1)]
        }
    }
    fn name(&self) -> String {
        format!("Generic image with template {}", self.url_template)
    }
}

impl std::fmt::Debug for ZoomLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Generic level")
    }
}


#[test]
fn test_generic_dezoomer() {
    let uri = "{{X}},{{Y}}".to_string();
    let mut lvl = GenericDezoomer {}
        .zoom_levels(&DezoomerInput {
            uri,
            contents: None,
        })
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let existing_tiles = vec!["0,0", "1,0", "2,0", "0,1", "1,1", "2,1"];

    let mut all_tiles = vec![];

    let mut zoom_level_iter = crate::dezoomer::ZoomLevelIter::new(&mut lvl);
    let mut tries = 0;
    while let Some(tiles) = zoom_level_iter.next_tile_references() {
        let count = tiles.len() as u64;

        let successes: Vec<_> = tiles
            .into_iter()
            .filter(|t| existing_tiles.contains(&t.url.as_str()))
            .collect();
        zoom_level_iter.set_fetch_result(TileFetchResult {
            count,
            successes: successes.len() as u64,
            tile_size: Some(Vec2d { x: 4, y: 5 }),
        });
        all_tiles.extend(successes);
        tries += 1;
        assert!(tries <= 10);
    };

    let expected = &[
        TileReference {
            url: "0,0".into(),
            position: Vec2d { x: 0, y: 0 },
        },
        TileReference {
            url: "1,0".into(),
            position: Vec2d { x: 4, y: 0 },
        },
        TileReference {
            url: "2,0".into(),
            position: Vec2d { x: 8, y: 0 },
        },
        TileReference {
            url: "0,1".into(),
            position: Vec2d { x: 0, y: 5 },
        },
        TileReference {
            url: "1,1".into(),
            position: Vec2d { x: 4, y: 5 },
        },
        TileReference {
            url: "2,1".into(),
            position: Vec2d { x: 8, y: 5 },
        },
    ];
    for tile in expected.iter() {
        assert!(all_tiles.contains(tile), "missing tile {:?} in {:?}", tile, all_tiles);
    }
}

#[test]
fn test_url_templating() {
    let url_template = "http://x.com/{{x:05}}_{{y}}".to_string();
    let lvl: ZoomLevel = ZoomLevel {
        url_template,
        dichotomy: Default::default(),
        last_tile: (0, 0),
        tile_size: None,
        done: Default::default(),
    };
    assert_eq!(lvl.tile_url_at(10, 11), "http://x.com/00010_11");
    assert_eq!(lvl.tile_url_at(123, 1), "http://x.com/00123_1");
}