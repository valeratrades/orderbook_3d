#![feature(trivial_bounds)]
use std::{str::FromStr, sync::atomic::{AtomicUsize, Ordering}};

use aggr_orderbook::{BookOrders, ListenBuilder, Market, Symbol};
use bevy::prelude::*;
use bevy_editor_pls::prelude::*;
use tokio::sync::mpsc;
use v_utils::io::{confirm, Percent};

static COUNTER: AtomicUsize = AtomicUsize::new(0);


#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();

	App::new()
		.add_plugins(DefaultPlugins)
		.add_plugins(EditorPlugin::default())
		.add_systems(Startup, setup)
		.add_systems(Update, write_frame).run();
}

#[derive(Debug, Resource)]
struct Receiver(mpsc::Receiver<BookOrders>);

fn write_frame(
	mut receiver: ResMut<Receiver>,
	mut commands: Commands,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut camera_query: Query<(&Camera, &mut Transform)>,
) {
	while let Ok(orders) = receiver.0.try_recv() {
		COUNTER.fetch_add(1, Ordering::Relaxed);
		let p: v_utils::io::Percent = Percent::from_str("0.02%").unwrap();
		let (bids, asks) = orders.to_plottable(Some(p), true); //dbg: want aggregate = false
		dbg!(&bids);

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

		(0..bids.0.len()).for_each(|i| {
			spawn_object(bids.0[i] - mid_price, bid_qties_log[i], (109.0, 0.97, 0.88));
		});
		(0..asks.0.len()).for_each(|i| {
			spawn_object(asks.0[i] - mid_price, ask_qties_log[i], (0.3, 0.5, 0.5));
		});

		let camera = camera_query.single_mut();
		let (_, mut transform) = camera;
		transform.translation = Vec3::new(0., 4., range * 1.5 + COUNTER.load(Ordering::SeqCst) as f32);

		confirm("Continue?"); //dbg
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

	std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    
    rt.block_on(async {
        book_listen(tx).await;
    });
});
}
