# Base Server

A robust, production-ready Rust server template designed to provide a solid foundation for building scalable web applications. This project comes pre-configured with essential boilerplate, authentication, and documentation tools.

## 🚀 Tech Stack

- **Framework**: [Axum](https://github.com/tokio-rs/axum) - Ergonmic and modular web framework.
- **ORM**: [Diesel](https://diesel.rs/) - Safe, extensible ORM and query builder.
- **Runtime**: [Tokio](https://tokio.rs/) - Asynchronous runtime for Rust.
- **Database**: PostgreSQL with [diesel-async](https://github.com/weiznich/diesel-async) and [bb8](https://github.com/djc/bb8) connection pooling.
- **API Docs**: [Utoipa](https://github.com/juhakivekas/utoipa) - Auto-generated OpenAPI documentation and Swagger UI.
- **Security**: Argon2 for password hashing and JWT for session management.
- **Email**: [Lettre](https://github.com/lettre/lettre) for sending emails with Handlebars templating.
- **Observability**: Tracing for structured logging and diagnostics.

## ✨ Features

- **Modular Architecture**: Clean separation between handlers, services, DTOs, and models.
- **Built-in Authentication**: Ready-to-use user registration and login flows.
- **Async Everything**: Leveraging Rust's async/await for high performance.
- **Automatic Documentation**: Interactive Swagger UI available at `/swagger-ui`.
- **Database Migrations**: Integrated Diesel migration support.
- **Environment Driven**: Configuration managed via `.env` files.
- **Production Ready**: Includes CORS, compression, and request tracing layers.

## 📂 Project Structure

```text
src/
├── auth/          # Authentication logic (JWT, middleware)
├── config.rs      # Configuration & environment management
├── dto/            # Data Transfer Objects (Request/Response schemas)
├── errors/         # Global error handling (ModuleError)
├── handlers/       # Route handlers and API logic
├── helpers/        # Utility functions (hashing, etc.)
├── macros/         # Custom procedural macros
├── mailer/         # Email service and templates
├── models/         # Database models and Diesel mappings
├── services/       # Business logic layer
├── schema.rs       # Diesel generated database schema
├── lib.rs          # Main library entry point
└── main.rs         # Application entry point & server setup
```

## 🛠️ Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (2024 edition)
- [PostgreSQL](https://www.postgresql.org/)
- [Diesel CLI](http://diesel.rs/guides/getting-started) (optional but recommended)

### Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/BukiOffor/base_server.git
   cd base_server
   ```

2. **Configure environment variables**:
   Create a `.env` file in the root directory:
   ```env
   DATABASE_URL=postgres://username:password@localhost/base_server
   JWT_SECRET=your_super_secret_key
   SMTP_HOST=smtp.example.com
   SMTP_PORT=587
   SMTP_USER=user@example.com
   SMTP_PASS=password
   ```

3. **Run database migrations**:
   The server runs migrations automatically on startup, but you can also run them manually:
   ```bash
   diesel migration run
   ```

4. **Start the server**:
   ```bash
   cargo run
   ```

The server will be running at `http://localhost:9000/api/v1`.

## 📜 API Documentation

Interactive API documentation is generated automatically. Once the server is running, visit:

- **Swagger UI**: [http://localhost:9000/swagger-ui](http://localhost:9000/swagger-ui)
- **OpenAPI JSON**: [http://localhost:9000/api-docs/openapi.json](http://localhost:9000/api-docs/openapi.json)

## 🤝 Contributing

This project is intended as a base template. Feel free to clone, modify, and build your own services on top of it. Contributions to improve the core boilerplate are always welcome!

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.
