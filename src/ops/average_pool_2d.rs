use core::array;

use libm::roundf;
use nalgebra::Const;

use crate::activation::FusedActivation;
use crate::activation::{relu, relu6};
use crate::buffer::Buffer2D;
use crate::quantize::Quantized;
use crate::tensor::Tensor4D;

// TODO: Performance evaluation (fine cast + iters vs bulk cast + nalgebra built-in)

pub struct AveragePool2DOptions {
    pub fused_activation: FusedActivation,
    pub padding: AveragePool2DPadding,
    pub strides: (usize, usize),
}

pub enum AveragePool2DPadding {
    SAME,
    VALID,
}

pub fn average_pool_2d<
    T: Quantized,
    const INPUT_ROWS: usize,
    const INPUT_COLS: usize,
    const INPUT_CHANS: usize,
    const FILTER_ROWS: usize,
    const FILTER_COLS: usize,
    const OUTPUT_ROWS: usize,
    const OUTPUT_COLS: usize,
>(
    input: Tensor4D<T, 1, INPUT_ROWS, INPUT_COLS, INPUT_CHANS, 1>,
    _filter_shape: (Const<FILTER_ROWS>, Const<FILTER_COLS>),
    output_scale: [f32; 1],
    output_zero_point: [T; 1],
    options: AveragePool2DOptions,
    constants: (f32, f32),
) -> Tensor4D<T, 1, OUTPUT_ROWS, OUTPUT_COLS, INPUT_CHANS, 1> {
    Tensor4D::new(
        [Buffer2D::from_fn(|i, j| {
            array::from_fn(|c| {
                let mut len = FILTER_ROWS * FILTER_COLS;
                let view: Buffer2D<T, FILTER_ROWS, FILTER_COLS> =
                    Buffer2D::from_fn(|m, n| match options.padding {
                        AveragePool2DPadding::SAME => {
                            let shift = ((FILTER_ROWS - 1) / 2, (FILTER_COLS - 1) / 2);
                            let index = (
                                if let Some(x) = (options.strides.0 * i + m).checked_sub(shift.0) {
                                    x
                                } else {
                                    len -= 1;
                                    return T::from_superset_unchecked(&0);
                                },
                                if let Some(x) = (options.strides.1 * j + n).checked_sub(shift.1) {
                                    x
                                } else {
                                    len -= 1;
                                    return T::from_superset_unchecked(&0);
                                },
                            );
                            if let Some(x) = input.buffer[0].get(index) {
                                x.get(c).copied().unwrap_or(x[0])
                            } else {
                                len -= 1;
                                T::from_superset_unchecked(&0)
                            }
                        }
                        AveragePool2DPadding::VALID => {
                            let x = input.buffer[0]
                                [(options.strides.0 * i + m, options.strides.1 * j + n)];
                            x.get(c).copied().unwrap_or(x[0])
                        }
                    });
                let x = 1. / len as f32 * view.cast::<i32>().sum() as f32;
                let y = T::from_superset_unchecked(&roundf(constants.0 * x + constants.1));
                match options.fused_activation {
                    FusedActivation::NONE => y,
                    FusedActivation::RELU => relu(y, output_zero_point[0]),
                    FusedActivation::RELU6 => relu6(y, output_scale[0], output_zero_point[0]),
                }
            })
        })],
        output_scale,
        output_zero_point,
    )
}

#[cfg(test)]
mod tests {
    use nalgebra::matrix;

    use super::*;

    const INPUT: Tensor4D<i8, 1, 2, 3, 2, 1> = Tensor4D {
        buffer: [matrix![
            [1, 2], [3, 4], [5, 6];
            [7, 8], [9, 10], [11, 12]
        ]],
        scale: [0.13],
        zero_point: [14],
    };
    const FILTER_SHAPE: (Const<2>, Const<3>) = (Const, Const);
    const OUTPUT_SCALE: [f32; 1] = [0.15];
    const OUTPUT_ZERO_POINT: [i8; 1] = [16];
    const OPTIONS: AveragePool2DOptions = AveragePool2DOptions {
        fused_activation: FusedActivation::NONE,
        padding: AveragePool2DPadding::SAME,
        strides: (1, 1),
    };
    const CONSTANTS: (f32, f32) = (0.866_666_7, 3.866_666_6);
    const OUTPUT: Tensor4D<i8, 1, 2, 3, 2, 1> = Tensor4D {
        buffer: [matrix![
            [8, 9], [9, 10], [10, 11];
            [11, 12], [12, 13], [13, 13]
        ]],
        scale: [0.15],
        zero_point: [16],
    };

    #[test]
    fn average_pool_2d_layer() {
        assert_eq!(
            average_pool_2d(
                INPUT,
                FILTER_SHAPE,
                OUTPUT_SCALE,
                OUTPUT_ZERO_POINT,
                OPTIONS,
                CONSTANTS,
            ),
            OUTPUT
        );
    }
}
