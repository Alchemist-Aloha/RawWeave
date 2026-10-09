//! Borrowed access to existing RGBA images and scene RGB, without copying input pixels.
use crate::{
    EvaluationContext, Inputs, NodeDescriptor, NodeError, NodeResult, PortDescriptor, Value,
};
use rawweave_color::SceneLinearRGB;
use rawweave_image::{Dimensions, Image, Region};

#[derive(Clone, Copy)]
pub enum RgbInput<'a> {
    Image(&'a Image),
    Scene(&'a SceneLinearRGB),
}

impl<'a> RgbInput<'a> {
    pub fn from_inputs(inputs: &'a Inputs) -> Result<Self, NodeError> {
        match (inputs.get("image"), inputs.get("scene")) {
            (Some(_), Some(_)) => Err(NodeError::Message(
                "connect either Image or Scene Linear RGB, not both".into(),
            )),
            (Some(Value::Image(image)), None) => Ok(Self::Image(image)),
            (None, Some(Value::SceneLinearRGB(scene))) => {
                if scene.pixels().len() > 64 * 1024 * 1024 {
                    return Err(NodeError::Message(
                        "scene exceeds the node pixel budget".into(),
                    ));
                }
                Ok(Self::Scene(scene))
            }
            (Some(_), None) => Err(NodeError::InvalidParameter("image".into())),
            (None, Some(_)) => Err(NodeError::InvalidParameter("scene".into())),
            (None, None) => Err(NodeError::MissingInput("image".into())),
        }
    }

    pub fn is_scene(self) -> bool {
        matches!(self, Self::Scene(_))
    }
    pub fn dimensions(self) -> Dimensions {
        match self {
            Self::Image(image) => image.dimensions(),
            Self::Scene(scene) => scene.dimensions(),
        }
    }
    pub fn width(self) -> u32 {
        self.dimensions().width
    }
    pub fn height(self) -> u32 {
        self.dimensions().height
    }
    pub fn origin(self) -> (u32, u32) {
        match self {
            Self::Image(image) => image.origin(),
            Self::Scene(_) => (0, 0),
        }
    }
    pub fn global_region(self) -> Region {
        match self {
            Self::Image(image) => image.global_region(),
            Self::Scene(scene) => {
                Region::new(0, 0, scene.dimensions().width, scene.dimensions().height)
            }
        }
    }
    pub fn coordinate_scale(self) -> u32 {
        match self {
            Self::Scene(scene) => scene.sampling().map_or(1, |sampling| 1_u32 << sampling.mip),
            Self::Image(_) => 1,
        }
    }
    /// Geometry-only outputs (masks) use the full image grid, not the sampled raster extent.
    pub fn full_region(self) -> Region {
        match self {
            Self::Scene(scene) => {
                let dimensions = scene
                    .sampling()
                    .map_or(scene.dimensions(), |sampling| sampling.full_dimensions);
                Region::new(0, 0, dimensions.width, dimensions.height)
            }
            Self::Image(image) => image.global_region(),
        }
    }
    pub fn pixel_global(self, x: u32, y: u32) -> Option<[f32; 4]> {
        match self {
            Self::Image(image) => image.pixel_global(x, y),
            Self::Scene(scene) => scene.pixel(x, y).map(|[r, g, b]| [r, g, b, 1.0]),
        }
    }
    pub fn pixel_full(self, x: u32, y: u32) -> Option<[f32; 4]> {
        if !self.full_region().contains(x, y) {
            return None;
        }
        let scale = self.coordinate_scale();
        match self {
            Self::Image(image) => image.pixel_global(x, y),
            Self::Scene(scene) => scene
                .pixel(x / scale, y / scale)
                .map(|[r, g, b]| [r, g, b, 1.0]),
        }
    }
    pub fn region(self, context: &EvaluationContext) -> Region {
        let full = self.global_region();
        if self.is_scene() {
            return full;
        }
        context.requested_region().map_or(full, |region| {
            region
                .intersection(full)
                .unwrap_or_else(|| Region::new(region.x, region.y, 0, 0))
        })
    }
    /// Collect output directly into its original representation; no RGBA scratch image for scenes.
    pub fn output(
        self,
        region: Region,
        pixels: impl IntoIterator<Item = [f32; 4]>,
    ) -> Result<Value, NodeError> {
        let count = region
            .dimensions()
            .pixel_count()
            .map_err(|e| NodeError::Message(e.to_string()))?;
        if count > 64 * 1024 * 1024 {
            return Err(NodeError::Message(
                "output exceeds the node pixel budget".into(),
            ));
        }
        match self {
            Self::Image(image) => Image::from_pixels_with_origin(
                region.dimensions(),
                (region.x, region.y),
                pixels.into_iter().collect(),
                image.pixel_format(),
                image.color_metadata(),
            )
            .map(Value::Image)
            .map_err(|e| NodeError::Message(e.to_string())),
            Self::Scene(scene) => {
                if region != self.global_region() {
                    return Err(NodeError::Message(
                        "scene outputs cannot represent a regional origin".into(),
                    ));
                }
                SceneLinearRGB::new(
                    region.dimensions(),
                    pixels.into_iter().map(|[r, g, b, _]| [r, g, b]).collect(),
                    scene.working_space(),
                )
                .and_then(|output| output.with_sampling(scene.sampling()))
                .map(Value::SceneLinearRGB)
                .map_err(|e| NodeError::Message(e.to_string()))
            }
        }
    }
    pub fn result(self, image_port: &str, output: Value) -> NodeResult {
        NodeResult::single(scene_output_id(image_port, self.is_scene()), output)
    }
    pub fn luminance_coefficients(self) -> Result<[f32; 3], NodeError> {
        match self {
            Self::Image(_) => Ok([0.2126, 0.7152, 0.0722]), // Preserve the existing Image algorithm.
            Self::Scene(scene) => scene
                .working_space()
                .luminance_coefficients()
                .map_err(|e| NodeError::Message(e.to_string())),
        }
    }
}

pub fn scene_output_id(image_port: &str, scene: bool) -> String {
    if !scene {
        image_port.into()
    } else if image_port == "image" {
        "scene".into()
    } else {
        format!("{image_port}_scene")
    }
}

/// Add explicitly typed alternatives while keeping all existing port IDs and types.
pub fn with_scene_ports(mut descriptor: NodeDescriptor) -> NodeDescriptor {
    if let Some(image) = descriptor.inputs.iter_mut().find(|port| port.id == "image") {
        image.required = false;
    }
    descriptor.inputs.push(PortDescriptor::input(
        "scene",
        "Scene Linear RGB",
        "color.SceneLinearRGB",
        false,
    ));
    let outputs: Vec<_> = descriptor
        .outputs
        .iter()
        .filter(|port| port.data_type == "core.Image")
        .map(|port| {
            PortDescriptor::output(
                scene_output_id(&port.id, true),
                if port.id == "image" {
                    "Scene Linear RGB".into()
                } else {
                    format!("{} (Scene Linear RGB)", port.name)
                },
                "color.SceneLinearRGB",
            )
        })
        .collect();
    descriptor.outputs.extend(outputs);
    descriptor
}
