# Echoes the v3 pipeline's dimension-derived `TF_VAR_*` injection
# (`cubtera_app::run::RunUseCase::build_resolution`) back out as outputs,
# so an end-to-end `cubtera plan`/`cubtera apply` can assert real variable
# wiring, not just "the process exited 0".

variable "dim_dome" {
  type = string
}

variable "unit_name" {
  type = string
}

output "dim_dome_echo" {
  value = var.dim_dome
}

output "unit_name_echo" {
  value = var.unit_name
}
