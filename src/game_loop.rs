use crate::d3d12::D3D12Layer;
use crate::InitParams;

pub struct GameLoop {
    d3d12_layer: D3D12Layer,
}

impl GameLoop {
    pub fn new(params: InitParams) -> Self {
        GameLoop {
            d3d12_layer: D3D12Layer::new(params),
        }
    }

    pub fn tick(&mut self) {
        // TODO: update()
        self.render();
    }

    fn render(&mut self) {
        self.d3d12_layer.prepare();

        self.d3d12_layer.clear();

        self.d3d12_layer.present();
    }
}
