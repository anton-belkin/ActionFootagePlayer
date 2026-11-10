# afp-catalog's watcher, for a NAS (docs/library-management.md §9, "The NAS's watcher"): keeps a project's catalog on
# the NAS's own disks, told by the system what changes. Built from this folder alone:
#   docker build -t afp-catalog catalog/
#   docker run -d --name afp-catalog-trip --restart unless-stopped --hostname nas \
#     -v "/volume1/ssd/Projects/Trip:/projects/Trip" -v /volume1/video:/shares/video:ro -v /home/me/afp-catalog/trip:/data \
#     --entrypoint sh afp-catalog -c 'umask 000 && exec afp-catalog watch "/projects/Trip" --share video=/shares/video'
# The project mounted under its own folder's name (its sources' markers are named after it, <name>.afplocation); as
# root (UGOS refuses inotify on root's folders to other users), what it writes open to all (umask 000)
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
