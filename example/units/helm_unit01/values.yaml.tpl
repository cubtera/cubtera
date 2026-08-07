test:
  - some_key: {{ dim_dc_meta.parent }}
  - vpc_cidr: {{ dim_dc_meta.vpc_cidr }}
{{#each dim_service_meta.owners }}
  - {{this}}
{{/each}}