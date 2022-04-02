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
docker run -e STEAM_USERNAME=<...> -e STEAM_PASSWORD=<...> \
    -e HEALTHCHECKS_URL=<...> -v distance-wr-log:/data -v steam:/root/.steam \
    distance-wr-log-backend
```

- Steam Guard should be disabled for the account used here, as it will block the login otherwise.
- `HEALTHCHECKS_URL` is optional, and accepts a [healthchecks.io](https://healthchecks.io/) ping url.
- The container persists the main data in `/data`, including the `changelist.json` which is read by the frontend.
- Binding the container's `/root/.steam` is optional but recommended, as it avoids Steam performing a 200MB+ update each time the container starts.
