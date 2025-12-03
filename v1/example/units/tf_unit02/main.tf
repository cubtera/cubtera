terraform {
  required_providers {
    local = {
      source = "hashicorp/local"
    }
  }
}

# Define variables
variable "file_content" {
  type    = string
  default = "This is a sample file created by Terraform."
}

variable "directory_name" {
  type    = string
  default = "terraform_created_dir"
}

# Create a local file
resource "local_file" "example_file" {
  content  = var.file_content
  filename = "${path.module}/example.txt"
}

# Create a directory
resource "local_file" "example_directory" {
  content  = ""
  filename = "${path.module}/${var.directory_name}/.keep"

  provisioner "local-exec" {
    command = "mkdir -p ${path.module}/${var.directory_name}"
  }
}

# Optional variables from manifest
variable "change_my_ip" {
    type = string
    default = "default_change_my_ip"
}

variable "my_home" {
    type = string
    default = "default_home"
}


# Output the file path
output "file_path" {
  value = local_file.example_file.filename
}

# Output the directory path
output "directory_path" {
  value = "${path.module}/${var.directory_name}"
}

output "home" {
  value = var.my_home
}

# Dimension values
output "dim_dc_name" {
  value = var.dim_dc_name
}

output "dim_dc_meta" {
  value = var.dim_dc_meta
}