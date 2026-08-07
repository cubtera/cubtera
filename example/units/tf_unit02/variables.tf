variable "unit_name" {
    description = "Name of the unit"
    type        = string
    default     = null
}

variable "org_name" {
    description = "Description of the unit"
    type        = string
    default     = null
}

variable "dim_tree" {
  description = "Description of the unit"
  type        = string
  default     = null
}

variable "dim_kids" {
  description = "Description of the unit"
  type        = string
  default     = null
}

output "dim_kids" {
  value = var.dim_kids
}

variable "tf_state_s3bucket" {
  description = "Description of the unit"
  type        = string
  default     = null
}

variable "tf_state_s3key" {
  description = "Description of the unit"
  type        = string
  default     = null
}

variable "tf_state_s3region" {
  description = "Description of the unit"
  type        = string
  default     = null
}
