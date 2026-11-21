use egui::{ColorImage, Context, TextureHandle, TextureOptions};

const DARK_PNG: &[u8] = include_bytes!("../../../../brand/notwig-dark.png");
const LIGHT_PNG: &[u8] = include_bytes!("../../../../brand/notwig-light.png");

pub(crate) fn icon(ctx: &Context, dark_mode: bool) -> TextureHandle {
    let id = egui::Id::new("notwig-brand-icon").with(dark_mode);
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let (bytes, name) = if dark_mode {
        (DARK_PNG, "notwig-brand-dark")
    } else {
        (LIGHT_PNG, "notwig-brand-light")
    };
    let texture = load_texture(ctx, name, bytes);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

fn load_texture(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = image::load_from_memory(bytes)
        .expect("embedded Notwig brand asset should be a valid PNG")
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color, TextureOptions::LINEAR)
}

#[doc(hidden)]
pub mod test_api {
    pub fn icon_size(ctx: &egui::Context, dark_mode: bool) -> [usize; 2] {
        super::icon(ctx, dark_mode).size()
    }
}
