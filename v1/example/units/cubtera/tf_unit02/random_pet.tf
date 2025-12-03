resource "local_file" "hello_world" {
  content  = "Hello, World!"
  filename = "${path.module}/hello.txt"
}

resource "local_file" "random_pet" {
  content  = "Your pet name is: ${random_pet.pet.id}"
  filename = "${path.module}/pet.txt"
}

resource "random_pet" "pet" {
  length    = 2
  separator = "-"
}
