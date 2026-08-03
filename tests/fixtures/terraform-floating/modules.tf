module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"
}

module "local_mod" {
  source = "./modules/local"
}

module "git_mod" {
  source = "git::https://example.com/org/mod.git?ref=main"
}
