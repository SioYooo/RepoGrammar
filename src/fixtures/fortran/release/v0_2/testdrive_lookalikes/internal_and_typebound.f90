! Bounded negative fixture: internal procedures and type-bound sections.
module internal_and_typebound
  use testdrive
  implicit none

  type :: suite_type
    integer :: count = 0
  contains
    procedure :: test_bound => bound_impl
    procedure :: collect_counts
  end type suite_type

  abstract interface
    subroutine test_iface(error)
      import :: error_type
      type(error_type), allocatable, intent(out) :: error
    end subroutine test_iface
  end interface

contains

  subroutine bound_impl(self)
    class(suite_type) :: self
    self%count = self%count + 1
  end subroutine bound_impl

  subroutine collect_counts(self, error)
    class(suite_type), intent(in) :: self
    type(error_type), allocatable, intent(out) :: error
  end subroutine collect_counts

  subroutine runner()
    contains
    subroutine test_internal(error)
      type(error_type), allocatable, intent(out) :: error
    end subroutine test_internal
  end subroutine runner

end module internal_and_typebound
