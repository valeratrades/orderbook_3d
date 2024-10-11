#![feature(trivial_bounds)]
use std::{
	str::FromStr,
	sync::atomic::{AtomicUsize, Ordering},
};

use aggr_orderbook::{BookOrders, ListenBuilder, Market, Symbol};
use bevy::{
	color::palettes::css::WHITE,
	input::common_conditions::input_just_pressed,
	prelude::*,
	render::camera::PerspectiveProjection,
	window::PrimaryWindow,
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use tokio::sync::mpsc;
use v_utils::io::Percent;

static N_ROWS_DRAWN: AtomicUsize = AtomicUsize::new(0);
static RANGE_MULTIPLIER: f32 = 1.5;
static FONT: &str = "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf";


#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();

	App::new()
		.add_plugins(DefaultPlugins)
		.add_plugins(PanOrbitCameraPlugin)
		.add_systems(Startup, setup)
		.add_systems(Update, write_frame)
		.add_systems(Update, update_price_label)
		.add_systems(Update, center_camera.run_if(input_just_pressed(KeyCode::Escape)))
		.add_systems(Update, set_rotation_center.run_if(input_just_pressed(KeyCode::Space)))
		.run();
}

#[derive(Debug, Resource)]
struct Shared {
	receiver: mpsc::Receiver<BookOrders>,
	asks_material_handle: Handle<StandardMaterial>,
	bids_material_handle: Handle<StandardMaterial>,
	cuboid_mesh_handle: Handle<Mesh>,
	last_row_width: f32,
	last_row_height: f32,
	last_midprice: f32,
}

fn center_camera(mut commands: Commands, shared: ResMut<Shared>, mut camera_query: Query<(Entity, &mut PanOrbitCamera, &mut Transform)>) {
	let camera = camera_query.single_mut();
	let (entity, _, mut transform) = camera;

	transform.translation = Vec3::new(0., shared.last_row_height, shared.last_row_width * RANGE_MULTIPLIER + N_ROWS_DRAWN.load(Ordering::SeqCst) as f32);
	transform.look_at(Vec3::ZERO, Vec3::Y);

	commands.entity(entity).insert(PanOrbitCamera::default());
}

/// None if cursor is outside of the window or doesn't intersect with the Y0 plane
fn cursor_y0_intersection(window: &Window, camera_transform: &GlobalTransform, camera: &Camera) -> Option<Vec3> {
	let ray = match window.cursor_position().and_then(|cursor_pos| camera.viewport_to_world(camera_transform, cursor_pos)) {
		Some(ray) => ray,
		None => return None,
	};

	let distance_to_plane = match ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::default()) {
		Some(distance) => distance,
		None => return None,
	};

	let intersect_vec3 = ray.origin + ray.direction * distance_to_plane;
	Some(intersect_vec3)
}

/// Display the price at the point where the cursor is pointing
fn update_price_label(q_window: Query<&Window, With<PrimaryWindow>>, mut q_label: Query<&mut Text, With<Label>>, mut q_camera: Query<(&Camera, &GlobalTransform)>, shared: Res<Shared>) {
	let (camera, camera_transform) = q_camera.single_mut();
	let mut label = q_label.single_mut();
	if let Some(y0_intersection) = cursor_y0_intersection(q_window.single(), camera_transform, camera) {
		let price_under_cursor = shared.last_midprice + y0_intersection.x;
		label.sections[0] = format!("dbg: {}\n{price_under_cursor}", y0_intersection).into();
	}
}

fn set_rotation_center(commands: Commands, q_window: Query<&Window, With<PrimaryWindow>>, mut q_camera: Query<(Entity, &Camera, &mut PanOrbitCamera, &GlobalTransform)>) {
	let (camera_entity, camera, mut panorbit, camera_transform) = q_camera.single_mut();

	if let Some(y0_intersection) = cursor_y0_intersection(q_window.single(), camera_transform, camera) {
		panorbit.target_focus = y0_intersection;
		panorbit.force_update = true;

		//commands.entity(camera_entity).insert(PanOrbitCamera{
		//	transform: camera_transform.clone(),
		//	target_focus: new_focus_target,
		//	focus: new_focus_target,
		//	..default()
		//});
	}
}

fn write_frame(mut commands: Commands, mut shared: ResMut<Shared>, mut camera_query: Query<(&Camera, &mut Transform), With<PanOrbitCamera>>) {
	while let Ok(orders) = shared.receiver.try_recv() {
		let p: v_utils::io::Percent = Percent::from_str("0.02%").unwrap();
		let (bids, asks) = orders.to_plottable(Some(p), true, true);
		if bids.x.is_empty() && asks.x.is_empty() {
			eprintln!("[WARN] Empty orderbook slice. Consider increasing the depth.");
			continue;
		}
		N_ROWS_DRAWN.fetch_add(1, Ordering::Relaxed);

		shared.last_midprice = (bids.x[0] + asks.x[0]) / 2.;
		let range = asks.x[asks.x.len() - 1] - bids.x[bids.x.len() - 1];
		shared.last_row_width = range;

		// // it breaks if I move this after the closure def, because immutable borrow. What the fuck.
		let max_y_bids = bids.y.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
		let max_y_asks = asks.y.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
		shared.last_row_height = max_y_bids.max(*max_y_asks);
		//
		let mut spawn_object = |x: f32, y: f32, material_handle: Handle<StandardMaterial>| {
			commands.spawn(PbrBundle {
				mesh: shared.cuboid_mesh_handle.clone(),
				material: material_handle,
				transform: Transform::from_xyz(x, y / 2., N_ROWS_DRAWN.load(Ordering::SeqCst) as f32).with_scale(Vec3::new(range / 1000., y, 1.)), //HACK: x scale could be shared
				..default()
			});
		};

		(0..bids.x.len()).for_each(|i| {
			spawn_object(bids.x[i] - shared.last_midprice, bids.y[i], shared.bids_material_handle.clone());
		});
		(0..asks.x.len()).for_each(|i| {
			spawn_object(asks.x[i] - shared.last_midprice, asks.y[i], shared.asks_material_handle.clone());
		});

		let camera = camera_query.single_mut();
		let (_, mut transform) = camera;
		let current_camera_pos = transform.translation;
		if current_camera_pos.x == 0. {
			transform.translation = Vec3::new(0., shared.last_row_height, shared.last_row_width * RANGE_MULTIPLIER + N_ROWS_DRAWN.load(Ordering::SeqCst) as f32);
		}
	}
}

//? potentially integrate with async_tasks on bevy or whatever is the most semantically correct way to do this
async fn book_listen(tx: mpsc::Sender<BookOrders>) {
	let symbol = Symbol::new(Market::BinancePerp, "BTCUSDT".to_owned());
	ListenBuilder::new(symbol).data_dir("./examples/data/").listen(tx).await.unwrap();
	unreachable!();
}

#[derive(Debug, Default, Component)]
struct Ruler;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, asset_server: Res<AssetServer>) {
	let (tx, rx) = mpsc::channel(65536);
	let shared = Shared {
		receiver: rx,
		asks_material_handle: materials.add(Color::hsl(0.3, 0.5, 0.5)),
		bids_material_handle: materials.add(Color::hsl(109.0, 0.97, 0.88)),
		cuboid_mesh_handle: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
		last_row_width: 0.,
		last_row_height: 0.,
		last_midprice: 0.,
	};
	commands.insert_resource(shared);

	commands.insert_resource(AmbientLight {
		color: WHITE.into(),
		brightness: 100.0,
	});

	// camera
	commands.spawn((
		Camera3dBundle {
			transform: Transform::from_xyz(0., 0., 0.).looking_at(Vec3::ZERO, Vec3::Y),
			projection: PerspectiveProjection { far: 1000.0, ..default() }.into(),
			..default()
		},
		PanOrbitCamera::default(),
	));

	commands
		.spawn(NodeBundle {
			style: Style {
				width: Val::Percent(100.0),
				height: Val::Percent(100.0),
				justify_content: JustifyContent::SpaceBetween,
				..default()
			},
			..default()
		})
		.with_children(|parent| {
			parent.spawn((
				TextBundle::from_section(
					"target text",
					TextStyle {
						font: asset_server.load(FONT),
						..default()
					},
				),
				Label,
			));
		});

	std::thread::spawn(move || {
		let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

		rt.block_on(async {
			book_listen(tx).await;
		});
	});
}
