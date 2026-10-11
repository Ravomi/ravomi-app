use crate::msdf::{
    contour::Contour, edge_segment::EdgeSegment, shape::Shape, vector::Vector2, EdgeColor,
};

#[derive(Debug, Default)]
pub struct ContourBuilder {
    pixel_scale: f64,
    contour: Contour,
    point: Vector2,
    start: Vector2,
}

impl ContourBuilder {
    pub fn open_at(x: f64, y: f64, pixel_scale: f64) -> Self {
        Self {
            contour: Contour::new(),
            point: Vector2::new(x, y),
            start: Vector2::new(x, y),
            pixel_scale,
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        let point = Vector2::new(x, y);
        if point == self.point {
            return;
        }
        self.contour.add_edge(EdgeSegment::new_linear(
            self.point * self.pixel_scale,
            point * self.pixel_scale,
            EdgeColor::WHITE,
        ));
        self.point = point;
    }

    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        let cpoint = Vector2::new(cx, cy);
        let point = Vector2::new(x, y);
        self.contour.add_edge(EdgeSegment::new_quadratic(
            self.point * self.pixel_scale,
            cpoint * self.pixel_scale,
            point * self.pixel_scale,
            EdgeColor::WHITE,
        ));
        self.point = point;
    }

    pub fn curve_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        let c1point = Vector2::new(c1x, c1y);
        let c2point = Vector2::new(c2x, c2y);
        let point = Vector2::new(x, y);
        self.contour.add_edge(EdgeSegment::new_cubic(
            self.point * self.pixel_scale,
            c1point * self.pixel_scale,
            c2point * self.pixel_scale,
            point * self.pixel_scale,
            EdgeColor::WHITE,
        ));
        self.point = point;
    }

    pub fn close(mut self) -> Contour {
        if self.point.x != self.start.x || self.point.y != self.start.y {
            self.line_to(self.start.x, self.start.y);
        }
        self.contour
    }
}

#[derive(Debug, Default)]
pub struct ShapeBuilder {
    pub pixel_scale: f64,
    pub(crate) shape: Shape,
    pub(crate) contour: Option<ContourBuilder>,
}

impl ShapeBuilder {
    pub fn build(self) -> Shape {
        self.shape
    }
}

impl ttf_parser::OutlineBuilder for ShapeBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.contour.is_some() {
            panic!("Unexpected move_to");
        }

        self.contour = ContourBuilder::open_at(x as _, y as _, self.pixel_scale).into();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.contour
            .as_mut()
            .expect("Opened contour")
            .line_to(x as _, y as _);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.contour
            .as_mut()
            .expect("Opened contour")
            .quad_to(x1 as _, y1 as _, x as _, y as _);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.contour
            .as_mut()
            .expect("Opened contour")
            .curve_to(x1 as _, y1 as _, x2 as _, y2 as _, x as _, y as _);
    }

    fn close(&mut self) {
        self.shape
            .contours
            .push(self.contour.take().expect("Opened contour").close());
    }
}
