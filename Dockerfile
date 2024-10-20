ARG from_image=scratch
ARG bin_name=cubtera
FROM ubuntu:latest AS builder

USER root
WORKDIR /cubtera
COPY ./cubtera ./
RUN chmod +x /cubtera/$bin_name

FROM $from_image
COPY --from=builder ./cubtera/$bin_name /
EXPOSE 8000
USER 1000
CMD ["/cubtera"]