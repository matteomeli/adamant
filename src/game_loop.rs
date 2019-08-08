use crate::d3d12::D3D12Layer;
use crate::InitParams;

pub struct GameLoop {
    d3d12_layer: D3D12Layer,
}

impl GameLoop {
    pub fn new(params: InitParams) -> Self {
        // TODO: Ideally would need to insert hooks to call after device is created and
        // after swapchain (and other window size dependent resources) is created
        GameLoop {
            d3d12_layer: D3D12Layer::new(params),
        }
    }

    pub fn tick(&mut self) {
        // TODO: update()
        self.render();
    }

    pub fn on_window_size_changed(&mut self, width: i32, height: i32) {
        if self.d3d12_layer.on_window_size_changed(width, height) {
            // TODO: update window size dependent resources
        }
    }

    fn render(&mut self) {
        self.d3d12_layer.prepare();
        self.d3d12_layer.clear();
        self.d3d12_layer.present();
    }
}
