ARG from_image=scratch
FROM ubuntu:latest AS builder

USER root
WORKDIR /cubtera
COPY ./cubtera ./
RUN chmod +x /cubtera/cubtera

FROM $from_image
COPY --from=builder ./cubtera/cubtera /
EXPOSE 8000
USER 1000
CMD ["/cubtera"]