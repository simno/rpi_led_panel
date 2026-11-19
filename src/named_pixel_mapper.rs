use std::str::FromStr;

use crate::{
    error::InvalidPixelMapperError, pixel_mapper::PixelMapper, rgb_matrix::MatrixCreationError,
};

/// The direction in which [`NamedPixelMapperType::Mirror`] flips the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirrorDirection {
    /// Swap left and right.
    Horizontal,
    /// Swap top and bottom.
    Vertical,
}

/// The clockwise angle by which [`NamedPixelMapperType::Rotate`] turns the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rotation {
    /// Keep the display as it is.
    Deg0,
    /// A quarter turn clockwise.
    Deg90,
    /// A half turn.
    Deg180,
    /// A quarter turn counterclockwise.
    Deg270,
}

/// Enum representing different pixel mapping options for mapping the logical layout of your boards
/// to your physical arrangement. These options allow you to customize the mapping to match your unique setup.
///
/// These options can be used with the `--pixelmapper` flag to choose between different mappings.
///
/// You can apply multiple mappers in your configuration, and they will be applied in the order you specify.
/// For example, to first mirror the panels horizontally and then rotate the resulting screen,
/// you can use `--pixelmapper Mirror:H --pixelmapper Rotate:90`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NamedPixelMapperType {
    /// The "Mirror" mapper allows you to mirror the output either horizontally or vertically.
    /// Specify 'H' for horizontal mirroring or 'V' for vertical mirroring as a parameter after a colon.
    /// Example: `--pixelmapper Mirror:H`
    Mirror(MirrorDirection),
    /// The "Rotate" mapper allows you to rotate your screen by a specified angle in degrees.
    /// Specify the desired angle as a parameter after a colon. It needs to be a multiple of 90 and can be
    /// negative.
    /// Example: `--pixelmapper Rotate:90` for a 90-degree rotation.
    Rotate(Rotation),
    /// The `UMapper` represents a pixel mapping strategy where a long chain of display panels
    /// is arranged in a U-shape configuration. This arrangement allows for a single chain display
    /// with panels of double height but still utilizing only one data chain.
    ///
    /// In this U-shape configuration, the panels are bent around after half of them to continue below,
    /// creating a visually unified display. This arrangement is ideal for scenarios where you want to maximize
    /// the display area without increasing the number of data chains.
    ///
    /// `UMapper` takes no parameters.
    /// `--pixelmapper U-mapper`
    ///
    /// For example, a single chain display with four 32x32 panels like this:
    /// ```text
    ///    [<][<][<][<] }- Raspberry Pi connector
    /// ```
    ///
    /// can be arranged in this 64x64 U-shaped display:
    /// ```text
    ///    [<][<] }----- Raspberry Pi connector
    ///    [>][>]
    /// ```
    ///
    /// This U-shape configuration can also be applied to displays with multiple chains.
    /// For instance, an arrangement with two chains, each consisting of 8 panels, can be represented as follows:
    /// ```text
    ///   [<][<][<][<]  }--- Pi connector #1
    ///   [>][>][>][>]
    ///   [<][<][<][<]  }--- Pi connector #2
    ///   [>][>][>][>]
    /// ```
    UMapper,

    /// The `ChainLink` represents a pixel mapping strategy where a long chain
    /// of display panels is arranged in a linear, single-direction chain.
    ///
    /// For instance, a chain consisting of 12 panels, with 4 panels
    /// (`chain_length` = 4, `parallel` = 3) per connector, can be represented as follows:
    /// ```text
    ///   [>][>][>][>] [>][>][>][>] [>][>][>][>]
    ///    │            │            └─ Pi connector #3
    ///    │            └─ Pi connector #2
    ///    └─> Pi connector #1
    /// ```
    ChainLink,

    /// The `VMapper` arranges the parallel chains side by side and the panels of each chain on top of each
    /// other, like the `V-mapper` of rpi-rgb-led-matrix.
    ///
    /// Options, separated by commas:
    /// - `Z`: every other panel is mounted upside down so that the cables can be shorter.
    /// - `R`: the panels of every other chain (the right one of each pair) are mounted upside down.
    ///
    /// Example: `--pixelmapper V-mapper` or `--pixelmapper V-mapper:Z,R`
    VMapper {
        /// Flip every other panel of a chain.
        z: bool,
        /// Flip every other chain.
        flip_right: bool,
    },
}

impl FromStr for NamedPixelMapperType {
    type Err = InvalidPixelMapperError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((command, param)) = s.split_once(':') {
            match command {
                "Mirror" => match param {
                    "H" | "h" => Ok(Self::Mirror(MirrorDirection::Horizontal)),
                    "V" | "v" => Ok(Self::Mirror(MirrorDirection::Vertical)),
                    other => Err(InvalidPixelMapperError::new(format!(
                        "'{other}' is not valid. Mirror parameter should be either 'V' or 'H'"
                    ))),
                },
                "Rotate" => {
                    let Ok(angle) = param.parse::<i32>() else {
                        return Err(InvalidPixelMapperError::new(
                            "Rotation angle is missing or invalid",
                        ));
                    };
                    match angle.rem_euclid(360) {
                        0 => Ok(Self::Rotate(Rotation::Deg0)),
                        90 => Ok(Self::Rotate(Rotation::Deg90)),
                        180 => Ok(Self::Rotate(Rotation::Deg180)),
                        270 => Ok(Self::Rotate(Rotation::Deg270)),
                        _ => Err(InvalidPixelMapperError::new(format!(
                            "'{angle}' is not valid. Rotation needs to be a multiple of 90 degrees"
                        ))),
                    }
                }
                "V-mapper" => {
                    let (mut z, mut flip_right) = (false, false);
                    for option in param.split(',').map(str::trim).filter(|o| !o.is_empty()) {
                        match option {
                            "Z" | "z" => z = true,
                            "R" | "r" => flip_right = true,
                            other => {
                                return Err(InvalidPixelMapperError::new(format!(
                                    "'{other}' is not a valid V-mapper option. Valid options are 'Z' and 'R'"
                                )));
                            }
                        }
                    }
                    Ok(Self::VMapper { z, flip_right })
                }
                other => Err(InvalidPixelMapperError::new(format!(
                    "'{other}' is not a valid Pixel mapping."
                ))),
            }
        } else if s == "V-mapper" {
            Ok(Self::VMapper {
                z: false,
                flip_right: false,
            })
        } else if s == "U-mapper" {
            Ok(Self::UMapper)
        } else if s == "ChainLink" {
            Ok(Self::ChainLink)
        } else {
            Err(InvalidPixelMapperError::new(format!(
                "'{s}' is not a valid Pixel mapping."
            )))
        }
    }
}

impl NamedPixelMapperType {
    pub(crate) fn create(
        self,
        chain: usize,
        parallel: usize,
    ) -> Result<Box<dyn PixelMapper>, MatrixCreationError> {
        match self {
            NamedPixelMapperType::Mirror(direction) => {
                Ok(Box::new(MirrorPixelMapper { direction }))
            }
            NamedPixelMapperType::Rotate(rotation) => Ok(Box::new(RotatePixelMapper { rotation })),
            NamedPixelMapperType::UMapper => Ok(Box::new(UArrangeMapper::new_with_parameters(
                chain, parallel,
            )?)),
            NamedPixelMapperType::ChainLink => Ok(Box::new(ChainLinkMapper { parallel })),
            NamedPixelMapperType::VMapper { z, flip_right } => Ok(Box::new(VerticalMapper {
                chain,
                parallel,
                z,
                flip_right,
            })),
        }
    }
}

struct MirrorPixelMapper {
    direction: MirrorDirection,
}

impl PixelMapper for MirrorPixelMapper {
    fn size_mapping(
        &self,
        matrix_width: usize,
        matrix_height: usize,
    ) -> Result<[usize; 2], MatrixCreationError> {
        Ok([matrix_width, matrix_height])
    }

    fn map_visible_to_matrix(
        &self,
        matrix_width: usize,
        matrix_height: usize,
        x: usize,
        y: usize,
    ) -> [usize; 2] {
        match self.direction {
            MirrorDirection::Horizontal => [matrix_width - 1 - x, y],
            MirrorDirection::Vertical => [x, matrix_height - 1 - y],
        }
    }
}

struct RotatePixelMapper {
    rotation: Rotation,
}

impl PixelMapper for RotatePixelMapper {
    fn size_mapping(
        &self,
        matrix_width: usize,
        matrix_height: usize,
    ) -> Result<[usize; 2], MatrixCreationError> {
        match self.rotation {
            Rotation::Deg0 | Rotation::Deg180 => Ok([matrix_width, matrix_height]),
            Rotation::Deg90 | Rotation::Deg270 => Ok([matrix_height, matrix_width]),
        }
    }

    fn map_visible_to_matrix(
        &self,
        matrix_width: usize,
        matrix_height: usize,
        x: usize,
        y: usize,
    ) -> [usize; 2] {
        match self.rotation {
            Rotation::Deg0 => [x, y],
            Rotation::Deg90 => [matrix_width - y - 1, x],
            Rotation::Deg180 => [matrix_width - x - 1, matrix_height - y - 1],
            Rotation::Deg270 => [y, matrix_height - x - 1],
        }
    }
}

struct UArrangeMapper {
    parallel: usize,
}

impl UArrangeMapper {
    fn new_with_parameters(chain: usize, parallel: usize) -> Result<Self, MatrixCreationError> {
        if chain < 2 {
            let message = "UArrangeMapper: Chain length needs to be at least 2 for useful folding"
                .to_string();
            return Err(MatrixCreationError::PixelMapperError(message));
        }
        if !chain.is_multiple_of(2) {
            let message = "UArrangeMapper: Chain length needs to be divisible by 2.".to_string();
            return Err(MatrixCreationError::PixelMapperError(message));
        }
        Ok(Self { parallel })
    }
}

impl PixelMapper for UArrangeMapper {
    fn size_mapping(
        &self,
        matrix_width: usize,
        matrix_height: usize,
    ) -> Result<[usize; 2], MatrixCreationError> {
        let visible_width = matrix_width / 2;
        let visible_height = 2 * matrix_height;
        if !matrix_height.is_multiple_of(self.parallel) {
            let message = format!(
                "UArrangeMapper: For parallel={} we would expect the \
                height={matrix_height} to be divisible by {}.",
                self.parallel, self.parallel
            );
            return Err(MatrixCreationError::PixelMapperError(message));
        }
        Ok([visible_width, visible_height])
    }

    fn map_visible_to_matrix(
        &self,
        matrix_width: usize,
        matrix_height: usize,
        x: usize,
        y: usize,
    ) -> [usize; 2] {
        let panel_height = matrix_height / self.parallel;
        let visible_width = matrix_width / 2;
        let slab_height = 2 * panel_height; // one folded u-shape
        let base_y = (y / slab_height) * panel_height;
        let y_in_slab = y % slab_height;

        let [matrix_x, matrix_y] = if y_in_slab < panel_height {
            // Upper panel of the slab
            [(x + matrix_width / 2), y_in_slab]
        } else {
            // Lower panel of the slab
            [(visible_width - x - 1), (slab_height - y_in_slab - 1)]
        };

        [matrix_x, base_y + matrix_y]
    }
}

struct ChainLinkMapper {
    parallel: usize,
}

impl PixelMapper for ChainLinkMapper {
    fn size_mapping(
        &self,
        matrix_width: usize,
        matrix_height: usize,
    ) -> Result<[usize; 2], MatrixCreationError> {
        Ok([matrix_width * self.parallel, matrix_height / self.parallel])
    }

    fn map_visible_to_matrix(
        &self,
        matrix_width: usize,
        matrix_height: usize,
        visible_x: usize,
        visible_y: usize,
    ) -> [usize; 2] {
        [
            visible_x % matrix_width,
            (matrix_height / self.parallel) * (visible_x / matrix_width) + visible_y,
        ]
    }
}

struct VerticalMapper {
    chain: usize,
    parallel: usize,
    z: bool,
    flip_right: bool,
}

impl PixelMapper for VerticalMapper {
    fn size_mapping(
        &self,
        matrix_width: usize,
        matrix_height: usize,
    ) -> Result<[usize; 2], MatrixCreationError> {
        if !matrix_width.is_multiple_of(self.chain) || !matrix_height.is_multiple_of(self.parallel)
        {
            return Err(MatrixCreationError::PixelMapperError(format!(
                "VerticalMapper: The {matrix_width}x{matrix_height} matrix can't be split into \
                {} panels wide and {} panels high.",
                self.chain, self.parallel
            )));
        }
        Ok([
            matrix_width / self.chain * self.parallel,
            matrix_height / self.parallel * self.chain,
        ])
    }

    fn map_visible_to_matrix(
        &self,
        matrix_width: usize,
        matrix_height: usize,
        x: usize,
        y: usize,
    ) -> [usize; 2] {
        let panel_width = matrix_width / self.chain;
        let panel_height = matrix_height / self.parallel;
        // Each column of panels is one parallel chain, and each row is one position in the chains.
        let chain_position = y / panel_height;
        let chain_index = x / panel_width;
        let x_within_panel = x % panel_width;
        let y_within_panel = y % panel_height;

        // The panel at the bottom is plugged in first and not flipped, so whether the top one is flipped
        // depends on the number of panels in the chain. This matches rpi-rgb-led-matrix.
        let z_flipped = self.z && self.chain % 2 == chain_position % 2;
        let right_flipped = self.flip_right && chain_index % 2 == 1;
        // Both flips are half turns, so they cancel each other out.
        let flipped = z_flipped != right_flipped;

        let [x_within_panel, y_within_panel] = if flipped {
            [
                panel_width - 1 - x_within_panel,
                panel_height - 1 - y_within_panel,
            ]
        } else {
            [x_within_panel, y_within_panel]
        };
        [
            chain_position * panel_width + x_within_panel,
            chain_index * panel_height + y_within_panel,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{MirrorDirection, NamedPixelMapperType, PixelMapper, Rotation, VerticalMapper};

    #[test]
    fn test_parse_v_mapper() {
        for (text, z, flip_right) in [
            ("V-mapper", false, false),
            ("V-mapper:", false, false),
            ("V-mapper:Z", true, false),
            ("V-mapper:z", true, false),
            ("V-mapper:R", false, true),
            ("V-mapper:R,Z", true, true),
            ("V-mapper:Z, R", true, true),
        ] {
            let mapper: NamedPixelMapperType = text.parse().unwrap();
            assert_eq!(
                mapper,
                NamedPixelMapperType::VMapper { z, flip_right },
                "{text}"
            );
        }
        assert!("V-mapper:X".parse::<NamedPixelMapperType>().is_err());
    }

    /// Check that the mapping is a bijection onto the matrix and return it as a lookup function.
    fn mapping(
        mapper: &VerticalMapper,
        width: usize,
        height: usize,
    ) -> impl Fn(usize, usize) -> [usize; 2] {
        let [visible_width, visible_height] = mapper.size_mapping(width, height).unwrap();
        assert_eq!(visible_width * visible_height, width * height);
        let mut seen = vec![false; width * height];
        for y in 0..visible_height {
            for x in 0..visible_width {
                let [mx, my] = mapper.map_visible_to_matrix(width, height, x, y);
                assert!(mx < width && my < height);
                assert!(!seen[my * width + mx], "({x}, {y}) maps to a pixel twice");
                seen[my * width + mx] = true;
            }
        }
        move |x, y| mapper.map_visible_to_matrix(width, height, x, y)
    }

    #[test]
    fn test_v_mapper_two_parallel_chains() {
        // Two 128x64 panels on two parallel chains, shown side by side as 256x64.
        let plain = VerticalMapper {
            chain: 1,
            parallel: 2,
            z: false,
            flip_right: false,
        };
        let map = mapping(&plain, 128, 128);
        assert_eq!(map(0, 0), [0, 0]);
        assert_eq!(map(128, 0), [0, 64]);
        assert_eq!(map(255, 63), [127, 127]);

        let flipped = VerticalMapper {
            chain: 1,
            parallel: 2,
            z: false,
            flip_right: true,
        };
        let map = mapping(&flipped, 128, 128);
        assert_eq!(map(0, 0), [0, 0]);
        assert_eq!(map(128, 0), [127, 127]);
        assert_eq!(map(255, 63), [0, 64]);
    }

    #[test]
    fn test_v_mapper_z() {
        // A chain of three 32x16 panels stacked vertically. The bottom panel isn't flipped.
        let mapper = VerticalMapper {
            chain: 3,
            parallel: 1,
            z: true,
            flip_right: false,
        };
        let map = mapping(&mapper, 96, 16);
        assert_eq!(map(0, 0), [0, 0]);
        assert_eq!(map(0, 16), [63, 15]);
        assert_eq!(map(0, 32), [64, 0]);

        // Both flips cancel out.
        let mapper = VerticalMapper {
            chain: 2,
            parallel: 2,
            z: true,
            flip_right: true,
        };
        let map = mapping(&mapper, 64, 32);
        assert_eq!(map(32, 0), [0, 16]);
    }

    #[test]
    fn test_parse_mirror() {
        let horizontal = NamedPixelMapperType::Mirror(MirrorDirection::Horizontal);
        let vertical = NamedPixelMapperType::Mirror(MirrorDirection::Vertical);
        assert_eq!(
            "Mirror:H".parse::<NamedPixelMapperType>().unwrap(),
            horizontal
        );
        assert_eq!(
            "Mirror:h".parse::<NamedPixelMapperType>().unwrap(),
            horizontal
        );
        assert_eq!(
            "Mirror:V".parse::<NamedPixelMapperType>().unwrap(),
            vertical
        );
        assert_eq!(
            "Mirror:v".parse::<NamedPixelMapperType>().unwrap(),
            vertical
        );
        assert!("Mirror:X".parse::<NamedPixelMapperType>().is_err());
    }

    #[test]
    fn test_parse_rotate() {
        for (angle, rotation) in [
            ("0", Rotation::Deg0),
            ("90", Rotation::Deg90),
            ("180", Rotation::Deg180),
            ("270", Rotation::Deg270),
            ("360", Rotation::Deg0),
            ("450", Rotation::Deg90),
            ("-90", Rotation::Deg270),
            ("-360", Rotation::Deg0),
        ] {
            let mapper: NamedPixelMapperType = format!("Rotate:{angle}").parse().unwrap();
            assert_eq!(mapper, NamedPixelMapperType::Rotate(rotation), "{angle}");
        }
        for angle in ["45", "-45", "", "up"] {
            let mapper = format!("Rotate:{angle}").parse::<NamedPixelMapperType>();
            assert!(mapper.is_err(), "{angle}");
        }
    }
}
