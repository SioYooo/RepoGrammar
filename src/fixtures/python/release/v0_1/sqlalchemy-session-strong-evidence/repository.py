from sqlalchemy.orm import Session


class UserRepository:
    def list_users(self, session: Session):
        return session.execute("select users")

    def list_accounts(self, session: Session):
        return session.execute("select accounts")

    def list_invoices(self, session: Session):
        return session.execute("select invoices")
