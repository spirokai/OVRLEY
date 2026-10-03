/** Attach Rust-owned resource identities at document ingress. */
export function attachRasterLoadResults(config, results) {
  return {
    ...config,
    rasters: config.rasters.map((raster) => {
      if (raster.path === null) return raster
      const resource = results[raster.id]
      if (!resource) throw new Error(`Raster ${raster.id} has no resource result`)
      if (resource.status === 'ready') return { ...raster, resourceId: resource.resourceId }
      if (resource.status === 'error') return { ...raster, resourceErrorCode: resource.errorCode }
      throw new Error(`Raster ${raster.id} has an invalid resource result`)
    }),
  }
}

/** Resolve only Rust handles when building a project archive. */
export function rasterResourceIds(config) {
  return Object.fromEntries(
    config.rasters
      .filter((raster) => raster.path !== null)
      .map((raster) => {
        if (!raster.resourceId) throw new Error(`Raster ${raster.id} has no loaded image resource`)
        return [raster.id, raster.resourceId]
      }),
  )
}
