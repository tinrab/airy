export RUSTFLAGS := "-Awarnings"

format:
    cargo fmt --all

lint:
    cargo fmt --all -- --check

    cargo clippy -- \
        -D warnings \
        -D unused_extern_crates \
        -D unused_import_braces \
        -D unused_qualifications \
        -D clippy::all \
        -D clippy::correctness \
        -D clippy::suspicious \
        -D clippy::complexity \
        -D clippy::perf \
        -D clippy::style

lint-fix:
    cargo clippy --fix --allow-dirty --allow-staged -- \
            -D warnings \
            -D unused_extern_crates \
            -D unused_import_braces \
            -D unused_qualifications \
            -D clippy::all \
            -D clippy::correctness \
            -D clippy::suspicious \
            -D clippy::complexity \
            -D clippy::perf \
            -D clippy::style

clean:
    cargo clean

docker-up:
    docker compose up -d --build

docker-down:
    docker compose down

migrate +args:
    docker run -v "$(pwd)"/tests/mysql/migrations:/migrations --network host migrate/migrate \
        -path=/migrations/ -database "mysql://employee:abc123456@tcp(127.0.0.1:3306)/employee" {{args}}

    docker run -v "$(pwd)"/tests/postgres/migrations:/migrations --network host migrate/migrate \
        -path=/migrations/ -database "postgres://employee:abc123456@127.0.0.1:5432/employee?sslmode=disable" {{args}}

mysql-download-data:
    #!/usr/bin/env -S bash -x

    REPO_OWNER="datacharmer"
    REPO_NAME="test_db"
    TAG="v1.0.7"
    BASE_URL="https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/${TAG}"

    FILES=(
        "load_departments.dump"
        "load_employees.dump"
        "load_dept_emp.dump"
        "load_dept_manager.dump"
        "load_titles.dump"
        "load_salaries1.dump"
        "load_salaries2.dump"
        "load_salaries3.dump"
        "show_elapsed.sql"
    )

    LOCAL_DATA_DIR="tests/mysql/data"

    mkdir -p "${LOCAL_DATA_DIR}"
    cd "${LOCAL_DATA_DIR}" || exit

    echo "Downloading files to $(pwd)..."

    for FILE in "${FILES[@]}"; do
        FILE_URL="${BASE_URL}/${FILE}"
        echo "Downloading ${FILE} from ${FILE_URL}..."
        if curl -s -L -O "${FILE_URL}"; then
            echo "${FILE} downloaded successfully."
        else
            echo "ERROR: Failed to download ${FILE}."
        fi
    done

    echo "All specified files have been downloaded."
    cd ..

mysql-load-data:
    #!/usr/bin/env -S bash -x

    docker run -v "$(pwd)"/tests/mysql/data:/data --network host --workdir /data mysql:9.3 mysql -h127.0.0.1 -P3306 -uemployee -pabc123456 employee -e "source load_data.sql"

postgres-download-data:
    #!/usr/bin/env -S bash -x

    REPO_OWNER="vrajmohan"
    REPO_NAME="pgsql-sample-data"
    COMMIT="b3f86c4415c85df60a4b678fe6fd4ada9947b693"
    BASE_URL="https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/${COMMIT}"

    FILES=(
        "employee/load_departments.sql"
        "employee/load_employees.sql"
        "employee/load_dept_emp.sql"
        "employee/load_dept_manager.sql"
        "employee/load_titles.sql"
        "employee/load_salaries.sql"
    )

    LOCAL_DATA_DIR="tests/postgres/data"

    mkdir -p "${LOCAL_DATA_DIR}"
    cd "${LOCAL_DATA_DIR}" || exit

    echo "Downloading files to $(pwd)..."

    for FILE in "${FILES[@]}"; do
        FILE_URL="${BASE_URL}/${FILE}"
        echo "Downloading ${FILE} from ${FILE_URL}..."
        if curl -s -L -O "${FILE_URL}"; then
            echo "${FILE} downloaded successfully."
        else
            echo "ERROR: Failed to download ${FILE}."
        fi
    done

    echo "All specified files have been downloaded."
    cd ..

postgres-load-data:
    #!/usr/bin/env -S bash -x

    docker run -v "$(pwd)"/tests/postgres/data:/data --network host --workdir /data --env PGPASSWORD=abc123456 postgres:17.5 psql -h127.0.0.1 -p5432 -Uemployee -W employee -f load_data.sql
