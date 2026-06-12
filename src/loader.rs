//! A minimal loading screen (`MARTIN_LOADER=1`, set automatically in a bundled build): a black
//! cover with the logo (`MARTIN_LOGO=<png OR svg in the asset root>`) and a slim progress bar that
//! tracks how many splats have loaded, then cross-fades into the show's opening logo. A `.svg` logo
//! is rasterized so it can be the *same* artwork the opening mesh was extruded from — a 1-to-1
//! loader→intro handoff. Off by default — dev runs skip it.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_gaussian_splatting::PlanarGaussian3d;

use crate::scene::AssetRoot;
use crate::scene::compose::Composition;
use crate::scene::sequence::SeqState;

#[derive(Component)]
struct LoaderRoot;

#[derive(Component)]
struct LoaderFill;

/// Tags every loader node (cover, logo, bar) so the lift-off fades them all together — the flat
/// loader logo dissolves into the show's opening 3D logo behind it (a seamless 1-to-1 handoff).
#[derive(Component)]
struct LoaderFade;

const FADE_OUT: f32 = 0.6; // loader cross-fade time (s) once the show is built
const MIN_SHOW: f32 = 0.8; // hold the loader at least this long so the logo registers before lift-off

/// Resolve `MARTIN_LOGO` to a texture handle: a `.svg` is rasterized (so it matches the mesh it was
/// extruded from), anything else is loaded as an image asset (PNG/JPEG). `None` if unset/unreadable.
fn logo_handle(
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    root: &std::path::Path,
) -> Option<Handle<Image>> {
    let logo = std::env::var("MARTIN_LOGO").ok()?;
    if logo.to_ascii_lowercase().ends_with(".svg") {
        let bytes = std::fs::read(root.join(&logo)).ok()?;
        let rgba = crate::splat_image::rasterize_svg(&bytes, 1024)?;
        let (w, h) = rgba.dimensions();
        let image = Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba.into_raw(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        Some(images.add(image))
    } else {
        Some(asset_server.load(logo))
    }
}

fn spawn_loader(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    root: Res<AssetRoot>,
) {
    let logo = logo_handle(&asset_server, &mut images, &root.0);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(22.0),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(1000),
            LoaderRoot,
            LoaderFade,
        ))
        .with_children(|p| {
            if let Some(logo) = logo {
                p.spawn((
                    Node {
                        width: Val::Px(480.0),
                        height: Val::Auto,
                        ..default()
                    },
                    ImageNode::new(logo),
                    LoaderFade,
                ));
            }
            // progress track + fill — a thin dim sliver; the logo is the star, and the show flows
            // OUT of it (the loader's logo → the demo's crisp logo mesh → splats).
            p.spawn((
                Node {
                    width: Val::Px(300.0),
                    height: Val::Px(3.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.10, 0.10, 0.13)),
                LoaderFade,
            ))
            .with_children(|track| {
                track.spawn((
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.88, 1.0)),
                    LoaderFill,
                    LoaderFade,
                ));
            });
        });
}

/// Drive the bar from splat-load progress; once the show is built, cross-fade the whole cover out
/// (revealing the show's opening logo behind it) and despawn it.
#[allow(clippy::too_many_arguments)]
fn update_loader(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<Assets<PlanarGaussian3d>>,
    state: Option<Res<SeqState>>,
    comp: Option<Res<Composition>>,
    mut fill: Query<&mut Node, With<LoaderFill>>,
    mut bg: Query<&mut BackgroundColor, With<LoaderFade>>,
    mut img: Query<&mut ImageNode, With<LoaderFade>>,
    root: Query<Entity, With<LoaderRoot>>,
    mut shown: Local<f32>, // total loader uptime (so the logo is seen even on an instant build)
    mut fade: Local<f32>,  // elapsed cross-fade time, accumulates once we start lifting off
) {
    *shown += time.delta_secs();
    let (loaded, total) = state
        .as_ref()
        .map(|s| {
            (
                s.loads.iter().filter(|h| assets.get(*h).is_some()).count(),
                s.loads.len().max(1),
            )
        })
        .unwrap_or((0, 1));
    let pct = (loaded as f32 / total as f32 * 100.0).clamp(0.0, 100.0);
    for mut node in &mut fill {
        node.width = Val::Percent(pct);
    }
    // Lift off once the show is built AND the logo has been up long enough to register (so the
    // cross-fade is visible even when the build is instant) — then fade the cover out.
    let built = state.map(|s| s.built).unwrap_or(false) || comp.map(|c| c.built).unwrap_or(false);
    if !built || *shown < MIN_SHOW {
        return;
    }
    *fade += time.delta_secs();
    let alpha = (1.0 - *fade / FADE_OUT).clamp(0.0, 1.0); // 1 → 0 over FADE_OUT
    for mut c in &mut bg {
        c.0.set_alpha(alpha);
    }
    for mut i in &mut img {
        i.color.set_alpha(alpha);
    }
    if alpha <= 0.0 {
        for e in &root {
            commands.entity(e).despawn();
        }
    }
}

/// The loading screen — only active when `MARTIN_LOADER` is set (bundled builds set it).
pub(crate) struct LoaderPlugin;

impl Plugin for LoaderPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("MARTIN_LOADER").is_some() {
            app.add_systems(Startup, spawn_loader)
                .add_systems(Update, update_loader);
        }
    }
}
