.PHONY: doc doc-serve doc-site clean-site

# Build the rustdoc API reference (target/doc/), including private items since
# macro_gen is a binary crate with no public API.
doc:
	cargo doc --no-deps --document-private-items

# Preview the mdBook usage guide locally at http://localhost:3000.
doc-serve:
	mdbook serve docs

# Assemble the combined docs site (mdBook at root, rustdoc under /api/) into ./site,
# mirroring what .github/workflows/docs.yml deploys to GitHub Pages.
doc-site: doc
	mdbook build docs
	rm -rf site
	mkdir -p site
	cp -r docs/book/. site/
	mkdir -p site/api
	cp -r target/doc/. site/api/
	echo '<meta http-equiv="refresh" content="0; url=macro_gen/index.html">' > site/api/index.html

clean-site:
	rm -rf site docs/book
