# afp-catalog's watcher, for a NAS (docs/library-management.md §9, "The NAS's watcher"): keeps a project's catalog on
# the NAS's own disks, told by the system what changes. Built from this folder alone:
#   docker build -t afp-catalog catalog/
#   docker run -d --name afp-catalog-trip --restart unless-stopped --hostname nas --user 1000:10 \
#     -v /volume1/ssd/Projects/Trip:/project -v /volume1/video:/shares/video -v afp-catalog-data:/data \
#     afp-catalog watch /project --share video=/shares/video
# 1.91: notify-types wants it (File::try_lock, used here, came in 1.89)
FROM rust:1.91-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin afp-catalog

FROM debian:bookworm-slim
COPY --from=build /src/target/release/afp-catalog /usr/local/bin/afp-catalog
# This computer's name and caches (keep it: a volume)
ENV AFP_HOME=/data
VOLUME /data
ENTRYPOINT ["afp-catalog"]
CMD ["--help"]
