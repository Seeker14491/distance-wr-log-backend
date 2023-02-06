# distance-wr-log-backend

The backend for a webapp that shows a log of new individual level world records for the game [Distance](http://survivethedistance.com/).

[Link to frontend repo](https://github.com/Seeker14491/distance-wr-log-frontend)

## Docker usage

### Building

```
docker build -t distance-wr-log-backend .
```

### Running

```
docker run -e GRPC_SERVER_ADDRESS=<...> -e STEAM_WEB_API_KEY=<...> \
    -e HEALTHCHECKS_URL=<...> -v distance-wr-log:/data \
    distance-wr-log-backend
```

- `HEALTHCHECKS_URL` is optional, and accepts a [healthchecks.io](https://healthchecks.io/) ping url.
- The container persists the main data in `/data`, including the `changelist.json` which is read by the frontend.
