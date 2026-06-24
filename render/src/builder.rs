use std::sync::mpsc::{self, Receiver, Sender};

use crate::{
    Mesh,
    mesher::{Material, build_mesh_geometry},
    render::MeshCpu,
};

struct MeshBuildJob {
    key: (i32, i32),
    voxels: Vec<u32>,
    size_xyz: [usize; 3],
}

pub struct MeshBuildResult {
    pub key: (i32, i32),
    pub mesh: MeshCpu,
}

pub struct VoxelMesher {
    job_tx: Sender<MeshBuildJob>,
    result_rx: Receiver<MeshBuildResult>,
}

impl Mesh {
    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}

impl MeshCpu {
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}

impl VoxelMesher {
    pub fn new(material_layers: Vec<[u32; 6]>) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<MeshBuildJob>();
        let (result_tx, result_rx) = mpsc::channel::<MeshBuildResult>();

        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let cpu_mesh = build_cpu_mesh(&job.voxels, job.size_xyz, &material_layers);

                if result_tx
                    .send(MeshBuildResult {
                        key: job.key,
                        mesh: cpu_mesh,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self { job_tx, result_rx }
    }

    pub fn enqueue(&self, key: (i32, i32), voxels: Vec<u32>, size_xyz: [usize; 3]) {
        let _ = self.job_tx.send(MeshBuildJob {
            key,
            voxels,
            size_xyz,
        });
    }

    pub fn collect_results(&mut self) -> Vec<MeshBuildResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }

        results
    }
}

fn build_cpu_mesh(
    voxels: &[Material],
    size_xyz: [usize; 3],
    material_layers: &[[u32; 6]],
) -> MeshCpu {
    let (vertices, indices) = build_mesh_geometry(voxels, size_xyz, material_layers);

    MeshCpu { vertices, indices }
}
