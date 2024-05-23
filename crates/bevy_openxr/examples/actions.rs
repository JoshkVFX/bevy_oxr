// a simple example showing basic actions

use std::borrow::Cow;

use bevy::{math::vec3, prelude::*};
use bevy_openxr::{
    action_binding::OxrSuggestActionBinding,
    action_set_attaching::OxrAttachActionSet,
    add_xr_plugins,
    helper_traits::ToQuat,
    init::OxrTrackingRoot,
    resources::{OxrInstance, OxrSession, OxrViews},
};
use openxr::{ActiveActionSet, Path, Vector2f};

fn main() {
    App::new()
        .add_plugins(add_xr_plugins(DefaultPlugins))
        .add_plugins(bevy_xr_utils::hand_gizmos::HandGizmosPlugin)
        .add_systems(Startup, setup_scene)
        .add_systems(Startup, create_action_entities)
        .add_systems(Startup, create_openxr_events.after(create_action_entities))
        .add_systems(Update, sync_active_action_sets)
        .add_systems(
            Update,
            sync_and_update_action_states_f32.after(sync_active_action_sets),
        )
        .add_systems(
            Update,
            sync_and_update_action_states_bool.after(sync_active_action_sets),
        )
        .add_systems(
            Update,
            sync_and_update_action_states_vector.after(sync_active_action_sets),
        )
        .add_systems(
            Update,
            read_action_with_marker_component.after(sync_and_update_action_states_f32),
        )
        .add_systems(
            Update,
            handle_flight_input.after(sync_and_update_action_states_f32),
        )
        .run();
}

/// set up a simple 3D scene
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // circular base
    commands.spawn(PbrBundle {
        mesh: meshes.add(Circle::new(4.0)),
        material: materials.add(Color::WHITE),
        transform: Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        material: materials.add(Color::rgb_u8(124, 144, 255)),
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..default()
    });
    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}

#[derive(Component)]
struct XRUtilsActionSet {
    name: Cow<'static, str>,
    pretty_name: Cow<'static, str>,
    priority: u32,
}

#[derive(Component, Clone)]
struct XRUtilsActionSetReference(openxr::ActionSet);

//I want to use this to indicate when an action set is attached
// #[derive(Component)]
// struct AttachedActionSet;

#[derive(Component)]
struct ActiveSet;

#[derive(Component)]
struct XRUtilsAction {
    action_name: Cow<'static, str>,
    localized_name: Cow<'static, str>,
    action_type: bevy_xr::actions::ActionType,
}

#[derive(Component)]
struct XRUtilsBinding {
    profile: Cow<'static, str>,
    binding: Cow<'static, str>,
}

#[derive(Component)]
struct Actionf32Reference {
    action: openxr::Action<f32>,
}

#[derive(Component)]
struct ActionBooleference {
    action: openxr::Action<bool>,
}

#[derive(Component)]
struct ActionVector2fReference {
    action: openxr::Action<Vector2f>,
}

#[derive(Component)]
struct FlightActionMarker;

fn create_action_entities(mut commands: Commands) {
    //create a set
    let set = commands
        .spawn((
            XRUtilsActionSet {
                name: "flight".into(),
                pretty_name: "pretty flight set".into(),
                priority: u32::MIN,
            },
            ActiveSet, //marker to indicate we want this synced
        ))
        .id();
    //create an action
    let action = commands
        .spawn((
            XRUtilsAction {
                action_name: "flight_input".into(),
                localized_name: "flight_input_localized".into(),
                action_type: bevy_xr::actions::ActionType::Vector,
            },
            FlightActionMarker, //lets try a marker component
        ))
        .id();

    //create a binding
    let binding = commands
        .spawn(XRUtilsBinding {
            profile: "/interaction_profiles/valve/index_controller".into(),
            binding: "/user/hand/right/input/thumbstick".into(),
        })
        .id();

    //add action to set, this isnt the best
    //TODO look into a better system
    commands.entity(action).add_child(binding);
    commands.entity(set).add_child(action);
}

fn create_openxr_events(
    action_sets_query: Query<(&XRUtilsActionSet, &Children, Entity)>,
    actions_query: Query<(&XRUtilsAction, &Children)>,
    bindings_query: Query<&XRUtilsBinding>,
    instance: ResMut<OxrInstance>,
    mut binding_writer: EventWriter<OxrSuggestActionBinding>,
    mut attach_writer: EventWriter<OxrAttachActionSet>,
    //not my favorite way of doing this
    mut commands: Commands,
) {
    //lets create some sets!
    //we gonna need a collection of these sets for later
    // let mut ActionSets = HashMap::new();
    for (set, children, id) in action_sets_query.iter() {
        //create action set
        let action_set: openxr::ActionSet = instance
            .create_action_set(&set.name, &set.pretty_name, set.priority)
            .unwrap();
        //now that we have the action set we need to put it back onto the entity for later
        let oxr_action_set = XRUtilsActionSetReference(action_set.clone());
        commands.entity(id).insert(oxr_action_set);

        //since the actions are made from the sets lets go
        for &child in children.iter() {
            //first get the action entity and stuff
            let (create_action, bindings) = actions_query.get(child).unwrap();
            //lets create dat actions
            match create_action.action_type {
                bevy_xr::actions::ActionType::Bool => {
                    let action: openxr::Action<bool> = action_set
                        .create_action::<bool>(
                            &create_action.action_name,
                            &create_action.localized_name,
                            &[],
                        )
                        .unwrap();
                    //please put this in a function so I dont go crazy
                    //insert a reference for later
                    commands.entity(child).insert((
                        ActionBooleference {
                            action: action.clone(),
                        },
                        XRUtilsActionState::Bool(ActionStateBool {
                            current_state: false,
                            changed_since_last_sync: false,
                            last_change_time: i64::MIN,
                            is_active: false,
                        }),
                    ));
                    //since we need actions for bindings lets go!!
                    for &bind in bindings.iter() {
                        //interaction profile
                        //get the binding entity and stuff
                        let create_binding = bindings_query.get(bind).unwrap();
                        let profile = Cow::from(create_binding.profile.clone());
                        //bindings
                        let binding = vec![Cow::from(create_binding.binding.clone())];
                        let sugestion = OxrSuggestActionBinding {
                            action: action.as_raw(),
                            interaction_profile: profile,
                            bindings: binding,
                        };
                        //finally send the suggestion
                        binding_writer.send(sugestion);
                    }
                }
                bevy_xr::actions::ActionType::Float => {
                    let action: openxr::Action<f32> = action_set
                        .create_action::<f32>(
                            &create_action.action_name,
                            &create_action.localized_name,
                            &[],
                        )
                        .unwrap();

                    //please put this in a function so I dont go crazy
                    //insert a reference for later
                    commands.entity(child).insert((
                        Actionf32Reference {
                            action: action.clone(),
                        },
                        XRUtilsActionState::Float(ActionStateFloat {
                            current_state: 0.0,
                            changed_since_last_sync: false,
                            last_change_time: i64::MIN,
                            is_active: false,
                        }),
                    ));
                    //since we need actions for bindings lets go!!
                    for &bind in bindings.iter() {
                        //interaction profile
                        //get the binding entity and stuff
                        let create_binding = bindings_query.get(bind).unwrap();
                        let profile = Cow::from(create_binding.profile.clone());
                        //bindings
                        let binding = vec![Cow::from(create_binding.binding.clone())];
                        let sugestion = OxrSuggestActionBinding {
                            action: action.as_raw(),
                            interaction_profile: profile,
                            bindings: binding,
                        };
                        //finally send the suggestion
                        binding_writer.send(sugestion);
                    }
                }
                bevy_xr::actions::ActionType::Vector => {
                    let action: openxr::Action<Vector2f> = action_set
                        .create_action::<Vector2f>(
                            &create_action.action_name,
                            &create_action.localized_name,
                            &[],
                        )
                        .unwrap();

                    //please put this in a function so I dont go crazy
                    //insert a reference for later
                    commands.entity(child).insert((
                        ActionVector2fReference {
                            action: action.clone(),
                        },
                        XRUtilsActionState::Vector(ActionStateVector {
                            current_state: [0.0, 0.0],
                            changed_since_last_sync: false,
                            last_change_time: i64::MIN,
                            is_active: false,
                        }),
                    ));
                    //since we need actions for bindings lets go!!
                    for &bind in bindings.iter() {
                        //interaction profile
                        //get the binding entity and stuff
                        let create_binding = bindings_query.get(bind).unwrap();
                        let profile = Cow::from(create_binding.profile.clone());
                        //bindings
                        let binding = vec![Cow::from(create_binding.binding.clone())];
                        let sugestion = OxrSuggestActionBinding {
                            action: action.as_raw(),
                            interaction_profile: profile,
                            bindings: binding,
                        };
                        //finally send the suggestion
                        binding_writer.send(sugestion);
                    }
                }
            };
        }

        attach_writer.send(OxrAttachActionSet(action_set));
    }
}

fn sync_active_action_sets(
    session: Res<OxrSession>,
    active_action_set_query: Query<&XRUtilsActionSetReference, With<ActiveSet>>,
) {
    let active_sets = active_action_set_query
        .iter()
        .map(|v| ActiveActionSet::from(&v.0))
        .collect::<Vec<_>>();
    let sync = session.sync_actions(&active_sets[..]);
    match sync {
        Ok(_) => info!("sync ok"),
        Err(_) => error!("sync error"),
    }
}

fn sync_and_update_action_states_f32(
    session: Res<OxrSession>,
    mut f32_query: Query<(&Actionf32Reference, &mut XRUtilsActionState)>,
) {
    //now we do the action state for f32
    for (reference, mut silly_state) in f32_query.iter_mut() {
        let state = reference.action.state(&session, Path::NULL);
        match state {
            Ok(s) => {
                info!("we found a state");
                let new_state = XRUtilsActionState::Float(ActionStateFloat {
                    current_state: s.current_state,
                    changed_since_last_sync: s.changed_since_last_sync,
                    last_change_time: s.last_change_time.as_nanos(),
                    is_active: s.is_active,
                });

                *silly_state = new_state;
            }
            Err(_) => {
                info!("error getting action state");
            }
        }
    }
}

fn sync_and_update_action_states_bool(
    session: Res<OxrSession>,
    mut f32_query: Query<(&ActionBooleference, &mut XRUtilsActionState)>,
) {
    //now we do the action state for f32
    for (reference, mut silly_state) in f32_query.iter_mut() {
        let state = reference.action.state(&session, Path::NULL);
        match state {
            Ok(s) => {
                info!("we found a state");
                let new_state = XRUtilsActionState::Bool(ActionStateBool {
                    current_state: s.current_state,
                    changed_since_last_sync: s.changed_since_last_sync,
                    last_change_time: s.last_change_time.as_nanos(),
                    is_active: s.is_active,
                });

                *silly_state = new_state;
            }
            Err(_) => {
                info!("error getting action state");
            }
        }
    }
}

fn sync_and_update_action_states_vector(
    session: Res<OxrSession>,
    mut vector_query: Query<(&ActionVector2fReference, &mut XRUtilsActionState)>,
) {
    //now we do the action state for f32
    for (reference, mut silly_state) in vector_query.iter_mut() {
        let state = reference.action.state(&session, Path::NULL);
        match state {
            Ok(s) => {
                info!("we found a state");
                let new_state = XRUtilsActionState::Vector(ActionStateVector {
                    current_state: [s.current_state.x, s.current_state.y],
                    changed_since_last_sync: s.changed_since_last_sync,
                    last_change_time: s.last_change_time.as_nanos(),
                    is_active: s.is_active,
                });

                *silly_state = new_state;
            }
            Err(_) => {
                info!("error getting action state");
            }
        }
    }
}

fn read_action_with_marker_component(
    mut action_query: Query<&XRUtilsActionState, With<FlightActionMarker>>,
) {
    //now for the actual checking
    for state in action_query.iter_mut() {
        info!("action state is: {:?}", state);
    }
}

//lets add some flycam stuff
fn handle_flight_input(
    action_query: Query<&XRUtilsActionState, With<FlightActionMarker>>,
    mut oxr_root: Query<&mut Transform, With<OxrTrackingRoot>>,
    time: Res<Time>,
    //use the views for hmd orientation
    views: ResMut<OxrViews>,
) {
    //now for the actual checking
    for state in action_query.iter() {
        // info!("action state is: {:?}", state);
        match state {
            XRUtilsActionState::Bool(_) => (),
            XRUtilsActionState::Float(_) => (),
            XRUtilsActionState::Vector(vector_state) => {
                //assuming we are mapped to a vector lets fly
                let input_vector = vec3(
                    vector_state.current_state[0],
                    0.0,
                    -vector_state.current_state[1],
                );
                //hard code speed for now
                let speed = 5.0;

                let root = oxr_root.get_single_mut();
                match root {
                    Ok(mut root_position) => {
                        //lets assume HMD based direction for now
                        let view = views.first();
                        match view {
                            Some(v) => {
                                let reference_quat = v.pose.orientation.to_quat();
                                let locomotion_vector = reference_quat.mul_vec3(input_vector);

                                root_position.translation +=
                                    locomotion_vector * speed * time.delta_seconds();
                            }
                            None => return,
                        }
                    }
                    Err(_) => {
                        info!("handle_flight_input: error getting root position for flight actions")
                    }
                }
            }
        }
    }
}

//the things i do for bad prototyping and lack of understanding
#[derive(Component, Debug)]
pub enum XRUtilsActionState {
    Bool(ActionStateBool),
    Float(ActionStateFloat),
    Vector(ActionStateVector),
}

#[derive(Debug)]
pub struct ActionStateBool {
    pub current_state: bool,
    pub changed_since_last_sync: bool,
    pub last_change_time: i64,
    pub is_active: bool,
}
#[derive(Debug)]
pub struct ActionStateFloat {
    pub current_state: f32,
    pub changed_since_last_sync: bool,
    pub last_change_time: i64,
    pub is_active: bool,
}
#[derive(Debug)]
pub struct ActionStateVector {
    pub current_state: [f32; 2],
    pub changed_since_last_sync: bool,
    pub last_change_time: i64,
    pub is_active: bool,
}
