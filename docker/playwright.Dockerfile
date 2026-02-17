FROM node:22-bookworm

# Fixed browser path so pre-installed browsers persist when tests/ is mounted
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

# Install npm dependencies and Playwright Chromium + system libraries at build time.
# This is the slow part (~60s) that gets cached in the image layer.
WORKDIR /deps
COPY tests/browser/package.json tests/browser/package-lock.json ./
RUN npm ci && npx playwright install --with-deps chromium

WORKDIR /work
