//! Damage and limits tests against the engine (the public API has no DOM mutation yet).

use std::path::Path;

use blitz_dom::{BaseDocument, LocalName, Namespace, QualName};

use crate::*;

const W: usize = 1024;
const H: usize = 600;

fn page_url(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testpages")
        .join(name);
    url::Url::from_file_path(std::path::absolute(path).unwrap())
        .unwrap()
        .to_string()
}

fn settle<N: Net>(b: &mut Browser<N>) {
    for _ in 0..1000 {
        b.tick();
        if b.is_idle() {
            return;
        }
    }
    panic!("page did not settle");
}

fn open(url: &str, config: Config) -> Browser<FileNet> {
    let mut b = Browser::new(config, FileNet::default());
    b.load(url);
    settle(&mut b);
    b
}

fn full_frame<N: Net>(b: &mut Browser<N>) -> Vec<u16> {
    b.invalidate();
    let mut fb = vec![0u16; W * H];
    b.render_raw(&mut fb, W);
    fb
}

fn qual(name: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(name))
}

/// Render the dashboard, mutate it, render incrementally, and check that the updated
/// framebuffer equals a fresh full render of the mutated page.
fn damage_case(mutate: impl FnOnce(&mut BaseDocument)) -> DamageRects {
    let mut b = open(&page_url("dashboard.html"), Config::default());
    let mut fb = vec![0u16; W * H];
    let first = b.render_raw(&mut fb, W);
    assert_eq!(first.area(), (W * H) as u32, "first render is full");
    let before = fb.clone();

    mutate(b.engine.doc_mut());
    settle(&mut b);
    let damage = b.render_raw(&mut fb, W);
    let fresh = full_frame(&mut b);

    let changed = (0..W * H).filter(|&i| before[i] != fresh[i]).count();
    assert!(changed > 0, "the mutation changed no pixels");
    let missed: Vec<_> = (0..W * H)
        .filter(|&i| fb[i] != fresh[i])
        .map(|i| (i % W, i / W))
        .collect();
    assert!(
        missed.is_empty(),
        "{} pixels differ from a full render, first at {:?}; damage {damage:?}",
        missed.len(),
        missed.first()
    );
    // Every changed pixel lies in a reported rect.
    for i in (0..W * H).filter(|&i| before[i] != fresh[i]) {
        let (x, y) = ((i % W) as u16, (i / W) as u16);
        assert!(
            damage.iter().any(|r| r.contains(x, y)),
            "({x},{y}) not in damage"
        );
    }
    damage
}

#[test]
fn class_toggle_redraws_only_the_card() {
    let damage = damage_case(|doc| {
        let card = doc.get_element_by_id("card-1").expect("card-1");
        doc.mutate().set_attribute(card, qual("class"), "card on");
    });
    let share = f64::from(damage.area()) / (W * H) as f64;
    assert!(
        share < 0.25,
        "damage covers {share:.2} of the screen: {damage:?}"
    );
}

#[test]
fn text_edit_redraws_only_the_text() {
    let damage = damage_case(|doc| {
        let span = doc.get_element_by_id("temp-4").expect("temp-4");
        let text = doc.get_node(span).unwrap().children[0];
        doc.mutate().set_node_text(text, "25.0\u{a0}°C");
    });
    let share = f64::from(damage.area()) / (W * H) as f64;
    assert!(
        share < 0.05,
        "damage covers {share:.3} of the screen: {damage:?}"
    );
}

#[test]
fn list_insertion_redraws_everything_it_shifts() {
    // M0: the new <li> grows the sidebar and resizes cards that carry no restyle damage;
    // only the bounds diff catches them.
    damage_case(|doc| {
        let list = doc.get_element_by_id("status").expect("status");
        let mut m = doc.mutate();
        let li = m.create_element(
            QualName::new(
                None,
                Namespace::from("http://www.w3.org/1999/xhtml"),
                LocalName::from("li"),
            ),
            vec![],
        );
        let text = m.create_text_node("Doorbell rang");
        m.append_children(li, &[text]);
        m.append_children(list, &[li]);
    });
}

#[test]
fn idle_page_renders_nothing() {
    let mut b = open(&page_url("dashboard.html"), Config::default());
    let mut fb = vec![0u16; W * H];
    assert!(!b.render_raw(&mut fb, W).is_empty());
    assert!(!b.tick());
    assert!(b.render_raw(&mut fb, W).is_empty());
}

struct Target(Vec<u16>);

impl embedded_graphics_core::geometry::OriginDimensions for Target {
    fn size(&self) -> embedded_graphics_core::prelude::Size {
        embedded_graphics_core::prelude::Size::new(W as u32, H as u32)
    }
}

impl embedded_graphics_core::draw_target::DrawTarget for Target {
    type Color = embedded_graphics_core::pixelcolor::Rgb565;
    type Error = std::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
    {
        use embedded_graphics_core::prelude::IntoStorage;
        for embedded_graphics_core::Pixel(p, c) in pixels {
            self.0[p.y as usize * W + p.x as usize] = c.into_storage();
        }
        Ok(())
    }
}

#[test]
fn draw_target_matches_raw_framebuffer() {
    let mut b = open(&page_url("links.html"), Config::default());
    let raw = full_frame(&mut b);
    b.invalidate();
    let mut t = Target(vec![0; W * H]);
    let damage = b.render(&mut t).unwrap();
    assert_eq!(
        damage.as_slice().len(),
        1,
        "full-width bands merge into one rect"
    );
    assert_eq!(t.0, raw);
    assert!(raw.iter().any(|&p| p != raw[0]), "blank frame");
}

fn document_text(b: &mut Browser<FileNet>) -> String {
    let doc = b.engine.doc_mut();
    doc.tree()
        .iter()
        .filter_map(|(_, n)| n.text_data().map(|t| t.content.to_string()))
        .collect()
}

#[test]
fn oversized_document_becomes_a_placeholder() {
    let mut config = Config::default();
    config.limits.document_bytes = 1024;
    let mut b = open(&page_url("dashboard.html"), config);
    assert!(document_text(&mut b).contains("larger than the 1 KB limit"));
}

#[test]
fn too_many_nodes_become_a_placeholder() {
    let mut config = Config::default();
    config.limits.dom_nodes = 50;
    let mut b = open(&page_url("dashboard.html"), config);
    assert!(document_text(&mut b).contains("more than the 50 limit"));
}

#[test]
fn oversized_images_become_placeholders() {
    // article.html embeds three 160x110 PNGs as data: URLs.
    let mut config = Config::default();
    config.limits.image_pixels = 100 * 100;
    let mut b = open(&page_url("article.html"), config);
    let doc = b.engine.doc_mut();
    let sizes: Vec<_> = doc
        .tree()
        .iter()
        .filter(|(_, n)| n.data.is_element_with_tag_name(&LocalName::from("img")))
        .filter_map(|(_, n)| {
            n.element_data()?
                .raster_image_data()
                .map(|i| (i.width, i.height))
        })
        .collect();
    assert!(!sizes.is_empty());
    assert!(sizes.iter().all(|&s| s == (1, 1)), "{sizes:?}");
}

#[test]
fn missing_page_shows_an_error_page() {
    let mut b = open(&page_url("does-not-exist.html"), Config::default());
    assert!(document_text(&mut b).contains("could not be loaded"));
}

#[test]
fn data_url_document_loads_without_net() {
    let mut b = open(
        "data:text/html,<p style='background:rgb(255,0,0);height:600px'>x</p>",
        Config::default(),
    );
    let fb = full_frame(&mut b);
    assert_eq!(fb[100 * W + 500], 0xf800, "red");
}
