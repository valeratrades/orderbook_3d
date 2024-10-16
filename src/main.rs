#![feature(trivial_bounds)]
use std::{
	str::FromStr,
	sync::atomic::{AtomicUsize, Ordering},
};

use aggr_orderbook::{BookOrders, ListenBuilder, Market, Symbol};
use bevy::{color::palettes::css::WHITE, input::common_conditions::input_just_pressed, prelude::*, render::camera::PerspectiveProjection, window::PrimaryWindow};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use tokio::sync::mpsc;
use v_utils::io::Percent;

static N_ROWS_DRAWN: AtomicUsize = AtomicUsize::new(0);
static TOTAL_POINTS_RENDERED: AtomicUsize = AtomicUsize::new(0);
static MAX_POINTS_RENDERED: usize = 250_000; // although it can work somewhat fine with up to 1_000_000
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
	first_row_properties: Option<RowProperties>,
	last_row_properties: RowProperties,
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
struct RowProperties {
	width: f32,
	height_2std_upper: f32,
	midprice: f32,
	n_orders: usize,
}
impl RowProperties {
	pub fn z_scale(&self) -> f32 {
		self.height_2std_upper / 4.
	}

	pub fn x_scale(&self) -> f32 {
		(self.n_orders as f32 + std::f32::consts::E).ln()
	}

	//? feels suboptimal. There must be a way to have this be more dynamic, and cover like up to 0.7, instead of having most variance come from the constant. Maybe I should have smaller log base?
	pub fn order_width_from_tick_size(&self, tick_size: f64) -> f32 {
		let log_n = (self.n_orders as f64).ln();
		(tick_size * log_n / (log_n + 1.0)) as f32 * 0.5 // scaling factor for easier visual separation
	}
}

fn __center_camera(commands: &mut Commands, shared: &ResMut<Shared>, entity: Entity, transform: &mut Transform) { 
	if let Some(first_row_properties) = shared.first_row_properties {
		let x_pos  = shared.last_row_properties.midprice - first_row_properties.midprice;
		transform.translation = Vec3::new(
			x_pos,
			shared.last_row_properties.height_2std_upper,
			shared.last_row_properties.width * RANGE_MULTIPLIER + N_ROWS_DRAWN.load(Ordering::SeqCst) as f32 * first_row_properties.z_scale(),
		);
		transform.look_at(Vec3::new(x_pos, 0., 0.), Vec3::Y);
		commands.entity(entity).insert(PanOrbitCamera::default());
	}
}

fn center_camera(mut commands: Commands, shared: ResMut<Shared>, mut q_camera: Query<(Entity, &mut Transform), With<PanOrbitCamera>>) {
		let (entity, mut transform) = q_camera.single_mut();
		__center_camera(&mut commands, &shared, entity, &mut transform);
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
	if let Some(row_properties) = shared.first_row_properties {
		let (camera, camera_transform) = q_camera.single_mut();
		let mut label = q_label.single_mut();
		match cursor_y0_intersection(q_window.single(), camera_transform, camera) {
			Some(y0_intersection) => {
				let price_under_cursor = row_properties.midprice + y0_intersection.x;
				label.sections[0] = format!("dbg: {}\n{price_under_cursor}", y0_intersection).into();
			}
			None => {
				label.sections[0] = "".into();
			}
		}
	}
}


fn set_rotation_center(commands: Commands, q_window: Query<&Window, With<PrimaryWindow>>, mut q_camera: Query<(&Camera, &mut PanOrbitCamera, &GlobalTransform)>) {
	let (camera, mut panorbit, camera_transform) = q_camera.single_mut();

	if let Some(y0_intersection) = cursor_y0_intersection(q_window.single(), camera_transform, camera) {
		panorbit.target_focus = y0_intersection;
		panorbit.force_update = true;

		// would need to take Entity in q_camera
		//commands.entity(camera_entity).insert(PanOrbitCamera{
		//	transform: camera_transform.clone(),
		//	target_focus: new_focus_target,
		//	focus: new_focus_target,
		//	..default()
		//});
	}
}

fn write_frame(mut commands: Commands, mut shared: ResMut<Shared>, mut q_camera: Query<(Entity, &mut Transform), With<PanOrbitCamera>>) {
	while let Ok(orders) = shared.receiver.try_recv() {
		if TOTAL_POINTS_RENDERED.load(Ordering::Relaxed) > MAX_POINTS_RENDERED {
			continue;
		}
		let p: v_utils::io::Percent = Percent::from_str("0.02%").unwrap();
		let (bids, asks) = orders.to_plottable(Some(p), false, false);
		N_ROWS_DRAWN.fetch_add(1, Ordering::Relaxed);
		TOTAL_POINTS_RENDERED.fetch_add(bids.x.len() + asks.x.len(), Ordering::Relaxed);
		if TOTAL_POINTS_RENDERED.load(Ordering::Relaxed) > MAX_POINTS_RENDERED {
			eprintln!("Rendered over {MAX_POINTS_RENDERED} points. No further data will be rendered.");
		}
		if bids.x.is_empty() && asks.x.is_empty() {
			eprintln!("[WARN] Empty orderbook slice. Consider increasing the depth.");
			continue;
		}

		let current_row_properties = {
			let mut sorted_ys: Vec<f32> = bids.y.clone();
			sorted_ys.extend(&asks.y);
			sorted_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
			let std2_upper_bound = sorted_ys[(sorted_ys.len() as f32 * 0.95445) as usize];
			let n_orders = bids.x.len() + asks.x.len();
			RowProperties {
				width: asks.x[asks.x.len() - 1] - bids.x[bids.x.len() - 1],
				height_2std_upper: std2_upper_bound,
				midprice: (bids.x[0] + asks.x[0]) / 2.,
				n_orders,
			}
		};

		let first_row_properties = match shared.first_row_properties {
			None => {
				shared.first_row_properties = Some(current_row_properties);
				current_row_properties
			}
			Some(row_properties) => row_properties,
		};
		shared.last_row_properties = current_row_properties;
		let z_scale = first_row_properties.z_scale();
		let x_scale = first_row_properties.x_scale();
		let order_width = first_row_properties.order_width_from_tick_size(orders.tick_size);

		let mut spawn_objects = |orders: aggr_orderbook::PlottableXy, material: &Handle<StandardMaterial>| {
			(0..orders.x.len()).for_each(|i| {
				let scaled_x_diff = (orders.x[i] - current_row_properties.midprice) * x_scale;
				let diff_from_init_midprice = current_row_properties.midprice - first_row_properties.midprice;
				let z_pos = N_ROWS_DRAWN.load(Ordering::SeqCst) as f32 * z_scale;

				commands.spawn(PbrBundle {
					mesh: shared.cuboid_mesh_handle.clone(),
					material: material.clone(),
					transform: Transform::from_xyz(scaled_x_diff - diff_from_init_midprice, orders.y[i] / 2.,z_pos).with_scale(Vec3::new(
						order_width * x_scale,
						orders.y[i],
						z_scale * 0.9, /*leave small gaps for visual separation*/
					)),
					..default()
				});
			});
		};

		spawn_objects(bids, &shared.bids_material_handle);
		spawn_objects(asks, &shared.asks_material_handle);

		let (entity, mut camera_transform) = q_camera.single_mut();
		let current_camera_pos = camera_transform.translation;
		// it's f32, so this gets off course very quickly
		if current_camera_pos.x == current_row_properties.midprice - first_row_properties.midprice {
			__center_camera(&mut commands, &shared, entity, &mut camera_transform);
		}
	}
}

//? potentially integrate with async_tasks on bevy or whatever is the most semantically correct way to do this
async fn book_listen(tx: mpsc::Sender<BookOrders>) {
	let symbol = Symbol::new(Market::BinancePerp, "BTCUSDT".to_owned());
	ListenBuilder::new(symbol).data_dir("./examples/data/").listen(tx).await.unwrap();
	unreachable!();
}

//#[derive(Debug, Default, Component)]
//struct Ruler;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, asset_server: Res<AssetServer>) {
	let (tx, rx) = mpsc::channel(65536);
	let shared = Shared {
		receiver: rx,
		asks_material_handle: materials.add(Color::hsl(0.3, 0.5, 0.5)),
		bids_material_handle: materials.add(Color::hsl(109.0, 0.97, 0.88)),
		cuboid_mesh_handle: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
		first_row_properties: None,
		last_row_properties: RowProperties::default(),
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
					"",
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_order_width_from_tick_size() {
		let tick_size = 0.1;
		let row_properties = RowProperties {
			n_orders: 100,
			..Default::default()
		};
		let result = row_properties.order_width_from_tick_size(tick_size);
		insta::assert_debug_snapshot!(result, @"0.054225158");
	}
}
