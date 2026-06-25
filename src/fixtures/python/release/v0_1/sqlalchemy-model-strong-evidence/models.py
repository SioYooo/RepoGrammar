from sqlalchemy.orm import Mapped


class User:
    __tablename__ = "users"

    id: Mapped[int]
    email: Mapped[str]


class Account:
    __tablename__ = "accounts"

    id: Mapped[int]
    owner_id: Mapped[int]


class Invoice:
    __tablename__ = "invoices"

    id: Mapped[int]
    account_id: Mapped[int]
