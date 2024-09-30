#![feature(trivial_bounds)]
use std::{str::FromStr, sync::Arc};

use tokio::sync::mpsc;
use aggr_orderbook::{Book, BookOrders, Market, Symbol};
use bevy::prelude::*;
use bevy_editor_pls::prelude::*;
use plotly::{Layout, Plot};
use tokio::task::JoinSet;
use v_utils::io::{confirm, Percent};

fn main() {
	color_eyre::install().unwrap();

	App::new()
		.add_plugins(DefaultPlugins)
		.add_plugins(EditorPlugin::default())
		.add_systems(Startup, setup)
		.add_systems(Update, update_data)
		.run();
}

#[derive(Debug, Resource)]
struct Receiver(mpsc::Receiver<BookOrders>);

async fn update_data(mut receiver: ResMut<Receiver>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut camera_query: Query<(&Camera, &mut Transform)>) {
	let frame = receiver.0.pop().unwrap();
	let p: v_utils::io::Percent = Percent::from_str("0.02%").unwrap();
	let (bids, asks) = frame.to_plottable(Some(p), false);

	fn to_log(v: Vec<f32>) -> Vec<f32> {
		let first = v[0]; //NB: smallest value for both, internal promise
		v.into_iter().map(|x: f32| (x / first).ln()).collect::<Vec<f32>>()
	}
	let ask_qties_log = to_log(asks.1);
	let bid_qties_log = to_log(bids.1);

	let mid_price = (bids.0[0] + asks.0[0]) / 2.;
	let range = asks.0[asks.0.len() - 1] - bids.0[bids.0.len() - 1];

	let mut spawn_object = |x: f32, y: f32, hsl: (f32, f32, f32)| {
		commands.spawn(PbrBundle {
			mesh: meshes.add(Cuboid::new(range / 1000., y, 1.0)),
			material: materials.add(Color::hsl(hsl.0, hsl.1, hsl.2)),
			transform: Transform::from_xyz(x, y / 2., 0.0),
			..default()
		});
	};

	for i in 0..bids.0.len() {
		spawn_object(bids.0[i] - mid_price, bid_qties_log[i], (109.0, 0.97, 0.88));
	}
	for i in 0..asks.0.len() {
		spawn_object(asks.0[i] - mid_price, ask_qties_log[i], (0.3, 0.5, 0.5));
	}

	//- move the camera back by 1.0

	//HACK: we spawn a new camera, can't update it after
	//commands.spawn(Camera3dBundle {
	//	transform: Transform::from_xyz(0., 4., range * 1.5).looking_at(Vec3::ZERO, Vec3::Y),
	//	..default()
	//});
	let camera = camera_query.single_mut();
	if let Some((_, mut transform)) = camera {
		transform.translation = Vec3::new(0., 4., range * 1.5);
	}

	confirm("Continue?");
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
	let (tx, rx) = mpsc::channel(65536);
	commands.insert_resource(Receiver(rx));

	// cube
	commands.spawn(PbrBundle {
		mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
		material: materials.add(Color::srgb_u8(124, 144, 255)),
		transform: Transform::from_xyz(0.0, 0.5, -1.0),
		..default()
	});

	// circle base
	commands.spawn(PbrBundle {
		mesh: meshes.add(Circle::new(999.0)),
		material: materials.add(Color::WHITE),
		transform: Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
		..default()
	});
	// light
	commands.spawn(PointLightBundle {
		point_light: PointLight { shadows_enabled: true, ..default() },
		transform: Transform::from_xyz(0.0, 8.0, 4.0),
		..default()
	});
	// camera
	commands.spawn(Camera3dBundle {
		transform: Transform::from_xyz(0., 0., 0.).looking_at(Vec3::ZERO, Vec3::Y),
		..default()
	});

	std::thread::scope(|s| {
		s.spawn(|| {
			let symbol = Symbol::new(Market::BinancePerp, "BTCUSDT".to_owned());
			Book::listen(symbol, tx).unwrap();
			unreachable!();
		});
	});
}
