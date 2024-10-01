#![feature(trivial_bounds)]
use std::{
	str::FromStr,
	sync::atomic::{AtomicUsize, Ordering},
};

use aggr_orderbook::{BookOrders, ListenBuilder, Market, Symbol};
use bevy::{color::palettes::css::WHITE, prelude::*, render::camera::PerspectiveProjection};
use bevy_editor_pls::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use iyes_perf_ui::entries::PerfUiBundle;
use tokio::sync::mpsc;
use v_utils::io::Percent;

static COUNTER: AtomicUsize = AtomicUsize::new(0);
//#[derive(Component)]
//struct MainCamera;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();

	App::new()
		.add_plugins(DefaultPlugins)
		.add_plugins(EditorPlugin::default())
		.add_plugins(PanOrbitCameraPlugin)
		.add_systems(Startup, setup)
		.add_systems(Update, write_frame)
		.run();
}

#[derive(Debug, Resource)]
struct Receiver(mpsc::Receiver<BookOrders>);

fn write_frame(
	mut receiver: ResMut<Receiver>,
	mut commands: Commands,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut camera_query: Query<(&Camera, &mut Transform), With<PanOrbitCamera>>,
) {
	while let Ok(orders) = receiver.0.try_recv() {
		let p: v_utils::io::Percent = Percent::from_str("0.02%").unwrap();
		let (bids, asks) = orders.to_plottable(Some(p), true, true);
		if bids.0.is_empty() && asks.0.is_empty() {
			eprintln!("[WARN] Empty orderbook slice. Consider increasing the depth.");
			continue;
		}
		COUNTER.fetch_add(1, Ordering::Relaxed);

		let mid_price = (bids.0[0] + asks.0[0]) / 2.;
		let range = asks.0[asks.0.len() - 1] - bids.0[bids.0.len() - 1];

		let mesh_handle = meshes.add(Cuboid::new(range / 1000., 1.0, 1.0));

		let mut spawn_object = |x: f32, y: f32, hsl: (f32, f32, f32)| {
			commands.spawn(PbrBundle {
				mesh: mesh_handle.clone(),
				material: materials.add(Color::hsl(hsl.0, hsl.1, hsl.2)),
				transform: Transform::from_xyz(x, y / 2., COUNTER.load(Ordering::SeqCst) as f32).with_scale(Vec3::new(1., y, 1.)),
				..default()
			});
		};

		(0..bids.0.len()).for_each(|i| {
			spawn_object(bids.0[i] - mid_price, bids.1[i], (109.0, 0.97, 0.88));
		});
		(0..asks.0.len()).for_each(|i| {
			spawn_object(asks.0[i] - mid_price, asks.1[i], (0.3, 0.5, 0.5));
		});

		let camera = camera_query.single_mut();
		let (_, mut transform) = camera;
		transform.translation = Vec3::new(0., 4., range * 1.5 + COUNTER.load(Ordering::SeqCst) as f32);
	}
}

//? potentially integrate with async_tasks on bevy or whatever is the most semantically correct way to do this
async fn book_listen(tx: mpsc::Sender<BookOrders>) {
	let symbol = Symbol::new(Market::BinancePerp, "BTCUSDT".to_owned());
	ListenBuilder::new(symbol).data_dir("./examples/data/").listen(tx).await.unwrap();
	unreachable!();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
	let (tx, rx) = mpsc::channel(65536);
	commands.insert_resource(Receiver(rx));

	// perf debug
	commands.spawn(PerfUiBundle::default());

	commands.insert_resource(AmbientLight {
		color: WHITE.into(),
		brightness: 100.0,
	});

	// camera
	commands.spawn((
		Camera3dBundle {
			transform: Transform::from_xyz(0., 0., 0.).looking_at(Vec3::ZERO, Vec3::Y),
			projection: PerspectiveProjection { far: 100.0, ..default() }.into(),
			..default()
		},
		PanOrbitCamera::default(),
	));

	std::thread::spawn(move || {
		let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

		rt.block_on(async {
			book_listen(tx).await;
		});
	});
}
