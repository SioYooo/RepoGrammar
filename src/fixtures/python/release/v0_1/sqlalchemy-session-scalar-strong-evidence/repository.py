from sqlalchemy.orm import Session


class UserRepository:
    def list_users(self, session: Session):
        return session.scalar("select users")

    def list_accounts(self, session: Session):
        return session.scalar("select accounts")

    def list_invoices(self, session: Session):
        return session.scalar("select invoices")
