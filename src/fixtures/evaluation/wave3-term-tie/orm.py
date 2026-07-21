from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class OrderRow(Base):
    __tablename__ = "orders"

    id: Mapped[int] = mapped_column(primary_key=True)
    total: Mapped[float] = mapped_column()


class InvoiceRow(Base):
    __tablename__ = "invoices"

    id: Mapped[int] = mapped_column(primary_key=True)
    total: Mapped[float] = mapped_column()


class PaymentRow(Base):
    __tablename__ = "payments"

    id: Mapped[int] = mapped_column(primary_key=True)
    total: Mapped[float] = mapped_column()


class ShipmentRow(Base):
    __tablename__ = "shipments"

    id: Mapped[int] = mapped_column(primary_key=True)
    total: Mapped[float] = mapped_column()


class RefundRow(Base):
    __tablename__ = "refunds"

    id: Mapped[int] = mapped_column(primary_key=True)
    total: Mapped[float] = mapped_column()


