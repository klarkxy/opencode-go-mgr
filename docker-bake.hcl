variable "SMOKE_IMAGE" {
  default = "ocg-manager:smoke"
}

variable "SMOKE_BROWSER_IMAGE" {
  default = "ocg-browser:smoke"
}

variable "CACHE_SCOPE" {
  default = "ocg-manager-linux-amd64"
}

variable "CACHE_BROWSER_SCOPE" {
  default = "ocg-browser-linux-amd64"
}

group "smoke" {
  targets = ["manager-smoke", "browser-smoke"]
}

# No explicit platforms: each native CI runner builds and loads its own
# architecture, which is what the per-architecture smoke suite expects.
target "manager-smoke" {
  context = "."
  dockerfile = "Dockerfile"
  tags = ["${SMOKE_IMAGE}"]
  cache-from = ["type=gha,scope=${CACHE_SCOPE}"]
}

target "browser-smoke" {
  context = "."
  dockerfile = "Dockerfile.browser"
  tags = ["${SMOKE_BROWSER_IMAGE}"]
  cache-from = ["type=gha,scope=${CACHE_BROWSER_SCOPE}"]
}
